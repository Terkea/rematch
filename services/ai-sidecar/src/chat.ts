import { existsSync } from "node:fs";
import { join } from "node:path";
import type { SDKMessage, SDKResultMessage, SessionStore } from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import type { SidecarConfig } from "./config.js";
import { internalError, timeoutError, upstreamError } from "./errors.js";
import type { Limiter } from "./limiter.js";
import type { LogFields, Logger, Metrics } from "./log.js";
import { buildOptions, type ChatRequest, PROJECT_DIR_NAME, type SessionChoice } from "./options.js";
import {
  apiErrorStatus,
  callController,
  mapResult,
  type QueryFn,
  RetryAfterTracker,
  usageFields,
  userPrompt,
} from "./query.js";
import { jsonSchemaToZodShape } from "./schema-to-zod.js";
import { type SseSink, SseWriter } from "./sse.js";
import { buildCoachServer } from "./tools.js";

/** The SDK store plus the two whole-session helpers the fallback needs. */
export type ChatSessionStore = SessionStore & {
  hasSession(sessionId: string): Promise<boolean>;
  deleteSession(sessionId: string): Promise<void>;
};

export type ChatDeps = {
  query?: QueryFn;
  limiter: Limiter;
  logger: Logger;
  metrics?: Metrics;
  /** Absent in development without a database: sessions then live on the local (scratch) disk. */
  store?: ChatSessionStore;
  pingMs?: number;
  fetchImpl?: typeof fetch;
};

export type SessionPath = "new" | "resumed" | "rebuilt";

type Attempt = {
  result?: SDKResultMessage;
  threw: boolean;
  emitted: boolean;
  retry: RetryAfterTracker;
};

const ROUTE = "/v1/chat";

function localTranscriptExists(cfg: SidecarConfig, id: string): boolean {
  return existsSync(join(cfg.configDir, "projects", PROJECT_DIR_NAME, `${id}.jsonl`));
}

/**
 * Runs one chat turn as SSE. Validation errors and queue rejections throw before any header is
 * written (the server turns them into JSON errors); everything after the headers ends the stream
 * with exactly one `done` or `error` event.
 */
export async function runChat(
  req: ChatRequest,
  cfg: SidecarConfig,
  res: SseSink,
  deps: ChatDeps,
): Promise<void> {
  // Reject an unsupported tool schema before anything is queued or spawned.
  for (const t of req.tools) jsonSchemaToZodShape(t.inputSchema);
  const query = deps.query ?? sdk.query;
  const client = new AbortController();
  let writer: SseWriter | undefined;
  res.on("close", () => {
    if (!writer?.done) client.abort();
  });
  const log: LogFields = { msg: "call", route: ROUTE, model: req.model };

  await deps.limiter.run(
    async () => {
      const w = new SseWriter(res);
      writer = w;
      w.open(deps.pingMs);
      const ctl = callController(req.timeoutMs, client.signal);

      const attempt = async (path: SessionPath): Promise<Attempt> => {
        const a: Attempt = { threw: false, emitted: false, retry: new RetryAfterTracker() };
        const session: SessionChoice =
          path === "resumed" ? { resume: req.conversationId } : { sessionId: req.conversationId };
        if (deps.store) session.store = deps.store;
        const blocks =
          path === "rebuilt" && req.rebuildContext
            ? [{ type: "text" as const, text: req.rebuildContext }, ...req.prompt]
            : req.prompt;
        try {
          const options = buildOptions(req, cfg, {
            abortController: ctl.ac,
            mcpServer: buildCoachServer(req.tools, req.toolCallback, deps.fetchImpl),
            toolNames: req.tools.map((t) => t.name),
            partial: true,
            session,
          });
          for await (const m of query({ prompt: userPrompt(blocks), options })) {
            a.retry.observe(m);
            if (forward(m, w)) a.emitted = true;
            if (m.type === "system" && (m as { subtype?: string }).subtype === "mirror_error") {
              deps.metrics?.mirrorError();
              deps.logger.warn({ msg: "mirror_error", route: ROUTE, event: "mirror_error" });
            }
            if (m.type === "result") a.result = m;
          }
        } catch {
          a.threw = true;
        }
        return a;
      };

      try {
        let path: SessionPath = "new";
        if (req.resume) {
          path = (await transcriptKnown(req.conversationId, cfg, deps.store))
            ? "resumed"
            : "rebuilt";
        }
        let a = await attempt(path);
        if (path === "resumed" && resumeFailed(a) && !ctl.timedOut() && !client.signal.aborted) {
          // The transcript exists but could not be resumed: clear it and start over, once.
          await deps.store?.deleteSession(req.conversationId).catch(() => {});
          path = "rebuilt";
          a = await attempt(path);
        }
        if (client.signal.aborted && !ctl.timedOut()) {
          deps.logger.info({ ...log, session: path, event: "client_gone" });
          return;
        }
        finish(a, path, ctl.timedOut(), w, deps.logger, log);
      } finally {
        ctl.dispose();
        w.close();
      }
    },
    { signal: client.signal },
  );
}

async function transcriptKnown(
  id: string,
  cfg: SidecarConfig,
  store: ChatSessionStore | undefined,
): Promise<boolean> {
  if (localTranscriptExists(cfg, id)) return true;
  if (!store) return false;
  // If the store cannot answer, try the resume anyway; its failure falls back below.
  return store.hasSession(id).catch(() => true);
}

function resumeFailed(a: Attempt): boolean {
  if (a.emitted) return false; // never replay a turn the client has partly seen
  if (!a.result) return a.threw;
  return a.result.subtype === "error_during_execution" && apiErrorStatus(a.result) === undefined;
}

/** Forwards top-level text deltas and tool_use blocks. Returns true when it wrote an event. */
function forward(m: SDKMessage, w: SseWriter): boolean {
  if (m.type === "stream_event") {
    if (m.parent_tool_use_id !== null) return false;
    const ev = m.event as { type?: string; delta?: { type?: string; text?: unknown } };
    if (ev.type === "content_block_delta" && ev.delta?.type === "text_delta") {
      w.event("delta", { text: String(ev.delta.text ?? "") });
      return true;
    }
    return false;
  }
  if (m.type === "assistant") {
    let wrote = false;
    const content = (m.message as { content?: unknown }).content;
    for (const b of Array.isArray(content) ? content : []) {
      const blk = b as { type?: string; id?: string; name?: string; input?: unknown };
      if (blk.type === "tool_use") {
        w.event("tool_use", { id: blk.id, name: blk.name, input: blk.input });
        wrote = true;
      }
    }
    return wrote;
  }
  return false;
}

function finish(
  a: Attempt,
  session: SessionPath,
  timedOut: boolean,
  w: SseWriter,
  logger: Logger,
  log: LogFields,
): void {
  const fail = (e: ReturnType<typeof internalError>) => {
    logger.warn({ ...log, session, status: e.status });
    w.finish("error", { ...e.body(), status: e.status });
  };
  if (timedOut || !a.result) {
    fail(timedOut ? timeoutError() : internalError());
    return;
  }
  const status = apiErrorStatus(a.result);
  if (status !== undefined) {
    fail(upstreamError(status, a.retry.header(status)));
    return;
  }
  const out = mapResult(a.result);
  logger.info({ ...log, session, status: 200, ...usageFields(out) });
  w.finish("done", { ...out, session });
}
