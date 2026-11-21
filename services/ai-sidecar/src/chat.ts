import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import type { SDKMessage, SDKResultMessage, SessionStore } from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import type { SidecarConfig } from "./config.js";
import { upstreamError } from "./errors.js";
import type { Limiter } from "./limiter.js";
import type { LogFields, Logger, Metrics } from "./log.js";
import { buildOptions, type ChatRequest, PROJECT_DIR_NAME, type SessionChoice } from "./options.js";
import {
  abortError,
  apiErrorStatus,
  callController,
  mapResult,
  type QueryFn,
  RetryAfterTracker,
  StreamUsage,
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
  /** Aborts the turn when the process shuts down (an `error` event with status 503). */
  shutdown?: AbortSignal;
  pingMs?: number;
  fetchImpl?: typeof fetch;
};

export type SessionPath = "new" | "resumed" | "rebuilt";

type Attempt = {
  result?: SDKResultMessage;
  threw: boolean;
  emitted: boolean;
  retry: RetryAfterTracker;
  usage: StreamUsage;
};

const ROUTE = "/v1/chat";

/** The CLI's local copy of a session: `<id>.jsonl` plus the `<id>/` subagent directory. */
function localTranscript(cfg: SidecarConfig, id: string): { file: string; dir: string } {
  const base = join(cfg.configDir, "projects", PROJECT_DIR_NAME);
  return { file: join(base, `${id}.jsonl`), dir: join(base, id) };
}

function removeLocalTranscript(cfg: SidecarConfig, id: string): void {
  const t = localTranscript(cfg, id);
  rmSync(t.file, { force: true });
  rmSync(t.dir, { recursive: true, force: true });
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
      const ctl = callController(req.timeoutMs, client.signal, deps.shutdown);
      const stream = new TurnStream(w, deps.logger);

      const attempt = async (path: SessionPath): Promise<Attempt> => {
        const a: Attempt = {
          threw: false,
          emitted: false,
          retry: new RetryAfterTracker(),
          usage: new StreamUsage(),
        };
        // A new session id must not exist on local disk, or the CLI reports it "already in use".
        if (path !== "resumed") removeLocalTranscript(cfg, req.conversationId);
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
            a.usage.observe(m);
            if (stream.forward(m)) a.emitted = true;
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

      let path: SessionPath = "new";
      try {
        if (req.resume) {
          path = (await transcriptKnown(req.conversationId, cfg, deps.store))
            ? "resumed"
            : "rebuilt";
        }
        let a = await attempt(path);
        if (path === "resumed" && resumeFailed(a) && ctl.cause() === undefined) {
          // The transcript exists but could not be resumed: clear it and start over, once.
          await deps.store?.deleteSession(req.conversationId).catch(() => {});
          path = "rebuilt";
          a = await attempt(path);
        }
        if (ctl.cause() === "client") {
          deps.logger.info({ ...log, session: path, event: "client_gone" });
          return;
        }
        finish(a, path, ctl.cause(), w, deps.logger, log);
      } finally {
        ctl.dispose();
        w.close();
        // Postgres holds the durable copy; the local transcript is scratch and is not kept.
        if (deps.store) {
          try {
            removeLocalTranscript(cfg, req.conversationId);
          } catch {
            deps.logger.warn({ msg: "local transcript cleanup failed", route: ROUTE });
          }
        }
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
  if (existsSync(localTranscript(cfg, id).file)) return true;
  if (!store) return false;
  // If the store cannot answer, try the resume anyway; its failure falls back below.
  return store.hasSession(id).catch(() => true);
}

function resumeFailed(a: Attempt): boolean {
  if (a.emitted) return false; // never replay a turn the client has partly seen
  if (!a.result) return a.threw;
  return a.result.subtype === "error_during_execution" && apiErrorStatus(a.result) === undefined;
}

/**
 * Turns SDK messages into SSE events. Deltas carry the API message id they belong to, so a
 * `retract` event can name the messages a refusal fallback (or a replaced, unparsable response)
 * withdrew; when a withdrawn message was never seen, the event says `all: true` (discard the
 * turn's text so far).
 */
class TurnStream {
  private current: string | undefined;
  private readonly idsByUuid = new Map<string, string>();

  constructor(
    private readonly w: SseWriter,
    private readonly logger: Logger,
  ) {}

  /** Returns true when it wrote a content event (delta or tool_use). */
  forward(m: SDKMessage): boolean {
    if (m.type === "stream_event") {
      if (m.parent_tool_use_id !== null) return false;
      const ev = m.event as {
        type?: string;
        message?: { id?: string };
        delta?: { type?: string; text?: unknown };
      };
      if (ev.type === "message_start") this.current = ev.message?.id;
      if (ev.type === "content_block_delta" && ev.delta?.type === "text_delta") {
        const data: { text: string; messageId?: string } = { text: String(ev.delta.text ?? "") };
        if (this.current) data.messageId = this.current;
        this.w.event("delta", data);
        return true;
      }
      return false;
    }
    if (m.type === "assistant") {
      const msg = m.message as { id?: string; content?: unknown };
      const supersedes = (m as { supersedes?: string[] }).supersedes;
      if (supersedes?.length) {
        this.logger.warn({ msg: "assistant message replaced", route: ROUTE, event: "superseded" });
        this.retract(supersedes, "superseded");
      }
      if (msg.id && m.parent_tool_use_id === null) this.idsByUuid.set(m.uuid, msg.id);
      let wrote = false;
      for (const b of Array.isArray(msg.content) ? msg.content : []) {
        const blk = b as { type?: string; id?: string; name?: string; input?: unknown };
        if (blk.type === "tool_use") {
          this.w.event("tool_use", { id: blk.id, name: blk.name, input: blk.input });
          wrote = true;
        }
      }
      return wrote;
    }
    if (m.type === "system") {
      const s = m as { subtype?: string; retracted_message_uuids?: string[] };
      if (s.subtype === "model_refusal_fallback") {
        this.logger.warn({
          msg: "model refusal fallback",
          route: ROUTE,
          event: "model_refusal_fallback",
        });
        this.retract(s.retracted_message_uuids ?? [], "model_refusal_fallback");
      }
    }
    return false;
  }

  private retract(uuids: string[], reason: string): void {
    const messageIds: string[] = [];
    let unknown = uuids.length === 0;
    for (const u of uuids) {
      const id = this.idsByUuid.get(u);
      if (id) messageIds.push(id);
      else unknown = true;
    }
    this.w.event("retract", unknown ? { messageIds, all: true, reason } : { messageIds, reason });
  }
}

function finish(
  a: Attempt,
  session: SessionPath,
  cause: ReturnType<ReturnType<typeof callController>["cause"]>,
  w: SseWriter,
  logger: Logger,
  log: LogFields,
): void {
  const fail = (e: ReturnType<typeof abortError>) => {
    logger.warn({ ...log, session, status: e.status });
    w.finish("error", { ...e.body(), status: e.status });
  };
  if (cause === "timeout" || cause === "shutdown" || !a.result) {
    fail(abortError(cause));
    return;
  }
  const status = apiErrorStatus(a.result);
  if (status !== undefined) {
    fail(upstreamError(status, a.retry.header(status)));
    return;
  }
  const out = mapResult(a.result);
  // The SDK's modelUsage is the running session total: on a resumed turn it includes every
  // earlier turn. Meter the turn's own usage from the stream instead. A new or rebuilt session
  // starts at zero in this query() call, so there the SDK's figure is the turn's own and is
  // preferred (it also counts internal calls the stream does not show).
  out.sessionModelUsage = out.modelUsage;
  if (session === "resumed") out.modelUsage = a.usage.totals();
  logger.info({ ...log, session, status: 200, ...usageFields(out) });
  w.finish("done", { ...out, session });
}
