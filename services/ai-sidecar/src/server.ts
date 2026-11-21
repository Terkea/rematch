import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { bearerOk } from "./auth.js";
import { type ChatDeps, runChat } from "./chat.js";
import type { SidecarConfig } from "./config.js";
import { ClientGone, errorBody, internalError, noCredentials, SidecarError } from "./errors.js";
import type { Limiter } from "./limiter.js";
import type { Logger, Metrics } from "./log.js";
import { parseChatRequest, parseQueryRequest } from "./options.js";
import { type QueryFn, runQuery } from "./query.js";

export const SDK_VERSION = "0.3.325";
const MAX_BODY = 4 * 1024 * 1024;

export type ServerDeps = {
  limiter: Limiter;
  logger: Logger;
  metrics: Metrics;
  query?: QueryFn;
  store?: ChatDeps["store"];
};

function sendJson(
  res: ServerResponse,
  status: number,
  body: unknown,
  headers: Record<string, string> = {},
) {
  const text = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json",
    "content-length": String(Buffer.byteLength(text)),
    ...headers,
  });
  res.end(text);
}

function sendError(res: ServerResponse, e: SidecarError) {
  if (res.headersSent) return;
  sendJson(res, e.status, e.body(), e.retryAfter ? { "retry-after": e.retryAfter } : {});
}

async function readJson(req: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const c of req) {
    size += (c as Buffer).length;
    if (size > MAX_BODY)
      throw new SidecarError(413, "invalid_request_error", "request body too large");
    chunks.push(c as Buffer);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    throw new SidecarError(400, "invalid_request_error", "body is not valid JSON");
  }
}

export function createSidecarServer(cfg: SidecarConfig, deps: ServerDeps): Server {
  const handle = async (req: IncomingMessage, res: ServerResponse) => {
    const path = (req.url ?? "/").split("?")[0];
    if (req.method === "GET" && path === "/health") {
      return sendJson(res, 200, {
        ok: true,
        auth: cfg.auth,
        ...deps.limiter.stats(),
        maxConcurrency: cfg.maxConcurrency,
        sdkVersion: SDK_VERSION,
        ...deps.metrics.snapshot(),
      });
    }
    if (!bearerOk(req.headers.authorization, cfg.token)) {
      return sendError(
        res,
        new SidecarError(401, "authentication_error", "missing or invalid bearer token"),
      );
    }
    const route = req.method === "POST" ? path : undefined;
    if (route !== "/v1/query" && route !== "/v1/chat") {
      return sendJson(res, 404, errorBody("not_found_error", "no such route"));
    }
    const body = await readJson(req);
    if (cfg.auth === "none") throw noCredentials();
    // The client going away aborts the call (and takes a queued request out of the queue).
    const client = new AbortController();
    res.on("close", () => {
      if (!res.writableFinished) client.abort();
    });
    if (route === "/v1/query") {
      const q = parseQueryRequest(body);
      const out = await runQuery(q, cfg, {
        limiter: deps.limiter,
        logger: deps.logger,
        signal: client.signal,
        ...(deps.query ? { query: deps.query } : {}),
      });
      return sendJson(res, 200, out);
    }
    const c = parseChatRequest(body);
    await runChat(c, cfg, res, {
      limiter: deps.limiter,
      logger: deps.logger,
      metrics: deps.metrics,
      ...(deps.query ? { query: deps.query } : {}),
      ...(deps.store ? { store: deps.store } : {}),
    });
  };

  return createServer((req, res) => {
    handle(req, res).catch((e: unknown) => {
      if (e instanceof ClientGone) {
        if (!res.writableEnded) res.destroy();
        return;
      }
      if (e instanceof SidecarError) return sendError(res, e);
      deps.logger.error({ msg: "unhandled", status: 500 });
      if (!res.headersSent) sendError(res, internalError());
      else if (!res.writableEnded) res.end();
    });
  });
}
