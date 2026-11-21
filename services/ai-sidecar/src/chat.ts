import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import type { SDKMessage, SDKResultMessage, SessionStore } from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import type { SidecarConfig } from "./config.js";
import { turnInProgress, upstreamError } from "./errors.js";
import type { Limiter } from "./limiter.js";
import type { LogFields, Logger, Metrics } from "./log.js";
import { buildOptions, type ChatRequest, PROJECT_DIR_NAME, type SessionChoice } from "./options.js";
import {
  abortError,
  addUsage,
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

/** The SDK store plus the whole-session helpers and the cross-replica turn lock. */
export type ChatSessionStore = SessionStore & {
  hasSession(sessionId: string): Promise<boolean>;
  deleteSession(sessionId: string): Promise<void>;
  /** Returns a release function, or null when another replica runs a turn of this conversation. */
  tryLockConversation(sessionId: string): Promise<(() => Promise<void>) | null>;
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
  /** Conversation ids with a turn running in this process (defaults to a process-wide set). */
  turns?: Set<string>;
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
const RUNNING_TURNS = new Set<string>();

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
 * Runs one chat turn as SSE. Validation errors, a concurrent turn of the same conversation (409)
 * and queue rejections throw before any header is written (the server turns them into JSON
 * errors); everything after the headers ends the stream with exactly one `done` or `error` event.
 *
 * One turn per conversation at a time: an in-process lock, plus a Postgres advisory lock when a
 * store is configured so replicas exclude each other. Two overlapping turns would otherwise delete
 * each other's transcripts and branch the stored session.
 */
export async function runChat(
  req: ChatRequest,
  cfg: SidecarConfig,
  res: SseSink,
  deps: ChatDeps,
): Promise<void> {
  // Reject an unsupported tool schema before anything is queued or spawned.
  for (const t of req.tools) jsonSchemaToZodShape(t.inputSchema);
  const turns = deps.turns ?? RUNNING_TURNS;
  const id = req.conversationId;
  if (turns.has(id)) throw turnInProgress();
  turns.add(id);
  try {
    await runLockedTurn(req, cfg, res, deps);
  } finally {
    turns.delete(id);
  }
}

async function runLockedTurn(
  req: ChatRequest,
  cfg: SidecarConfig,
  res: SseSink,
  deps: ChatDeps,
): Promise<void> {
  const query = deps.query ?? sdk.query;
  const id = req.conversationId;
  const client = new AbortController();
  let writer: SseWriter | undefined;
  res.on("close", () => {
    if (!writer?.done) client.abort();
  });
  const log: LogFields = { msg: "call", route: ROUTE, model: req.model };

  await deps.limiter.run(
    async () => {
      // The cross-replica lock is taken once a slot is free, so a queued request holds no
      // database connection. A refusal is still a 409 before any header.
      let unlock: (() => Promise<void>) | null = null;
      if (deps.store) {
        try {
          unlock = await deps.store.tryLockConversation(id);
        } catch {
          // Without the database the store cannot work either; the in-process lock still holds.
          deps.logger.warn({ msg: "turn lock unavailable", route: ROUTE });
          unlock = async () => {};
        }
        if (!unlock) throw turnInProgress();
      }
      try {
        await streamTurn(req, cfg, res, deps, query, client, log, (w) => {
          writer = w;
        });
      } finally {
        await unlock?.();
      }
    },
    { signal: client.signal },
  );
}

async function streamTurn(
  req: ChatRequest,
  cfg: SidecarConfig,
  res: SseSink,
  deps: ChatDeps,
  query: QueryFn,
  client: AbortController,
  log: LogFields,
  onWriter: (w: SseWriter) => void,
): Promise<void> {
  const id = req.conversationId;
  const w = new SseWriter(res);
  onWriter(w);
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
    if (path !== "resumed") removeLocalTranscript(cfg, id);
    const session: SessionChoice = path === "resumed" ? { resume: id } : { sessionId: id };
    if (deps.store) session.store = deps.store;
    const blocks =
      path === "rebuilt" && req.rebuildContext
        ? [{ type: "text" as const, text: req.rebuildContext }, ...req.prompt]
        : req.prompt;
    try {
      // A turn without tools (e.g. the "answer now" follow-up) mounts no coach server.
      const coach =
        req.toolCallback && req.tools.length > 0
          ? {
              mcpServer: buildCoachServer(req.tools, req.toolCallback, deps.fetchImpl),
              toolNames: req.tools.map((t) => t.name),
            }
          : {};
      const options = buildOptions(req, cfg, {
        abortController: ctl.ac,
        ...coach,
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
      path = (await transcriptKnown(id, cfg, deps.store)) ? "resumed" : "rebuilt";
    } else if (deps.store && (await deps.store.hasSession(id).catch(() => false))) {
      // A retried first turn: the earlier attempt left rows behind. Start the session clean.
      await deps.store.deleteSession(id).catch(() => {});
    }
    let a = await attempt(path);
    let failed: Attempt | undefined;
    if (path === "resumed" && resumeFailed(a) && ctl.cause() === undefined) {
      // The transcript exists but could not be resumed: clear it and start over, once.
      await deps.store?.deleteSession(id).catch(() => {});
      failed = a;
      path = "rebuilt";
      a = await attempt(path);
    }
    if (ctl.cause() === "client") {
      deps.logger.info({ ...log, session: path, event: "client_gone" });
      return;
    }
    finish(a, failed, path, ctl.cause(), w, deps.logger, log);
  } finally {
    ctl.dispose();
    w.close();
    // Postgres holds the durable copy; the local transcript is scratch and is not kept.
    if (deps.store) {
      try {
        removeLocalTranscript(cfg, id);
      } catch {
        deps.logger.warn({ msg: "local transcript cleanup failed", route: ROUTE });
      }
    }
  }
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
 * Turns SDK messages into SSE events. Deltas carry the API message id they belong to. A `retract`
 * names streamed messages the client must drop, following the SDK:
 * - a message whose stream ends with `stop_reason: "refusal"` is retracted at once, before the
 *   fallback model's replacement text starts;
 * - `supersedes` on an assistant message, and `retracted_message_uuids` on
 *   `model_refusal_fallback`, retract the named messages that were streamed and are not yet
 *   retracted. Unknown or already-retracted uuids are a no-op.
 * `done.text` is authoritative for the turn's final text.
 */
class TurnStream {
  private current: string | undefined;
  private readonly idsByUuid = new Map<string, string>();
  private readonly streamed = new Set<string>();
  private readonly retracted = new Set<string>();

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
        delta?: { type?: string; text?: unknown; stop_reason?: unknown };
      };
      if (ev.type === "message_start") this.current = ev.message?.id;
      if (ev.type === "message_delta" && ev.delta?.stop_reason === "refusal" && this.current) {
        this.retract([this.current], "refusal");
      }
      if (ev.type === "content_block_delta" && ev.delta?.type === "text_delta") {
        const data: { text: string; messageId?: string } = { text: String(ev.delta.text ?? "") };
        if (this.current) {
          data.messageId = this.current;
          this.streamed.add(this.current);
        }
        this.w.event("delta", data);
        return true;
      }
      return false;
    }
    if (m.type === "assistant") {
      const msg = m.message as { id?: string; content?: unknown };
      if (msg.id && m.parent_tool_use_id === null) this.idsByUuid.set(m.uuid, msg.id);
      const supersedes = (m as { supersedes?: string[] }).supersedes;
      if (supersedes?.length) {
        this.logger.warn({ msg: "assistant message replaced", route: ROUTE, event: "superseded" });
        this.retractUuids(supersedes, "superseded");
      }
      let wrote = false;
      for (const b of Array.isArray(msg.content) ? msg.content : []) {
        const blk = b as { type?: string; id?: string; name?: string; input?: unknown };
        if (blk.type === "tool_use") {
          this.w.event("tool_use", { id: blk.id, name: blk.name, input: blk.input });
          // A superseded message can withdraw its tool calls too, so they count as streamed.
          if (msg.id && m.parent_tool_use_id === null) this.streamed.add(msg.id);
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
        this.retractUuids(s.retracted_message_uuids ?? [], "model_refusal_fallback");
      }
    }
    return false;
  }

  private retractUuids(uuids: string[], reason: string): void {
    const ids: string[] = [];
    for (const u of uuids) {
      const id = this.idsByUuid.get(u);
      if (id) ids.push(id);
    }
    this.retract(ids, reason);
  }

  /** Emits one `retract` for the ids that were streamed and not yet retracted, if any. */
  private retract(ids: string[], reason: string): void {
    const fresh = [...new Set(ids)].filter((i) => this.streamed.has(i) && !this.retracted.has(i));
    if (fresh.length === 0) return;
    for (const i of fresh) this.retracted.add(i);
    this.w.event("retract", { messageIds: fresh, reason });
  }
}

function finish(
  a: Attempt,
  failed: Attempt | undefined,
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
  // `sessionModelUsage` is the SDK's running session total (on a resumed turn it includes every
  // earlier turn). `modelUsage` is this turn's own: the stream sum on a resumed turn, the SDK's
  // figure on a new or rebuilt one (it starts at zero and also counts internal calls), plus any
  // usage of a failed resume attempt that preceded a rebuild. See ADR-R3-6 for how Rust meters.
  out.sessionModelUsage = out.modelUsage;
  if (session === "resumed") out.modelUsage = a.usage.totals();
  if (failed) out.modelUsage = addUsage(out.modelUsage, failed.usage.totals());
  logger.info({ ...log, session, status: 200, ...usageFields(out) });
  w.finish("done", { ...out, session });
}
