import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { SDKMessage, SDKUserMessage } from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type ChatSessionStore, runChat } from "../src/chat.js";
import { SidecarError } from "../src/errors.js";
import { Limiter } from "../src/limiter.js";
import { Metrics } from "../src/log.js";
import { PROJECT_DIR_NAME, parseChatRequest } from "../src/options.js";
import {
  assistantToolUse,
  captureLogger,
  FakeCli,
  FakeRes,
  result,
  scripted,
  tempDir,
  testConfig,
  textDelta,
  usage,
} from "./helpers.js";

vi.mock("@anthropic-ai/claude-agent-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@anthropic-ai/claude-agent-sdk")>()),
  query: vi.fn(),
}));

const CONV = "6f1c3c1e-8d3b-4c1e-9a65-1f2b3c4d5e6f";
const baseBody = {
  model: "claude-sonnet-5-5",
  effort: "medium",
  maxTokens: 4000,
  timeoutMs: 600000,
  maxTurns: 6,
  systemPrompt: "SYS",
  prompt: [{ type: "text", text: "Why was 14...Nf6 a mistake?" }],
  conversationId: CONV,
  resume: false,
  tools: [
    {
      name: "get_position",
      description: "Position",
      inputSchema: {
        type: "object",
        properties: { gameId: { type: "string" } },
        required: ["gameId"],
        additionalProperties: false,
      },
    },
  ],
  toolCallback: { url: "http://127.0.0.1:9/v1/tools", token: "turn-token" },
};

function useScript(...scripts: Parameters<typeof scripted>[0]) {
  const s = scripted(scripts);
  vi.mocked(sdk.query).mockImplementation(s.fn as never);
  return s;
}

class FakeStore implements ChatSessionStore {
  rows = new Map<string, unknown[]>();
  deleted: string[] = [];
  async append(key: { sessionId: string }, entries: unknown[]) {
    this.rows.set(key.sessionId, [...(this.rows.get(key.sessionId) ?? []), ...entries]);
  }
  async load(key: { sessionId: string }) {
    return (this.rows.get(key.sessionId) as never) ?? null;
  }
  async hasSession(id: string) {
    return this.rows.has(id);
  }
  async deleteSession(id: string) {
    this.deleted.push(id);
    this.rows.delete(id);
  }
  /** Stands in for pg_try_advisory_lock: `held` simulates a turn running on another replica. */
  held = new Set<string>();
  async tryLockConversation(id: string) {
    if (this.held.has(id)) return null;
    this.held.add(id);
    return async () => {
      this.held.delete(id);
    };
  }
}

function deps() {
  return {
    limiter: new Limiter(4, 32, 30000),
    logger: captureLogger(),
    metrics: new Metrics(),
    store: new FakeStore(),
    turns: new Set<string>(),
  };
}

async function promptMessages(p: unknown): Promise<SDKUserMessage[]> {
  const out: SDKUserMessage[] = [];
  for await (const m of p as AsyncIterable<SDKUserMessage>) out.push(m);
  return out;
}

const terminal = (res: FakeRes) =>
  res.events().filter((e) => e.event === "done" || e.event === "error");

describe("runChat", () => {
  beforeEach(() => {
    vi.mocked(sdk.query).mockReset();
  });
  afterEach(() => vi.useRealTimers());

  it("streams_top_level_text_deltas_then_done", async () => {
    useScript([
      textDelta("Hello "),
      textDelta("SUBAGENT", "toolu_parent"),
      {
        type: "stream_event",
        event: {
          type: "content_block_delta",
          index: 0,
          delta: { type: "input_json_delta", partial_json: "{" },
        },
        parent_tool_use_id: null,
        uuid: "u",
        session_id: "s",
      } as unknown as SDKMessage,
      textDelta("world"),
      result({ result: "Hello world" }),
    ]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(res.status).toBe(200);
    expect(res.headers["content-type"]).toBe("text/event-stream");
    expect(res.events()).toEqual([
      { event: "delta", data: { text: "Hello " } },
      { event: "delta", data: { text: "world" } },
      {
        event: "done",
        data: expect.objectContaining({ subtype: "success", text: "Hello world", session: "new" }),
      },
    ]);
    expect(res.writableEnded).toBe(true);
  });

  it("tool_use_blocks_become_tool_use_events", async () => {
    const s = useScript([
      assistantToolUse("toolu_1", "mcp__coach__get_position", { gameId: "g1" }),
      result(),
    ]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(res.events()[0]).toEqual({
      event: "tool_use",
      data: { id: "toolu_1", name: "mcp__coach__get_position", input: { gameId: "g1" } },
    });
    const o = s.calls[0]?.options;
    expect(o?.allowedTools).toEqual(["mcp__coach__get_position"]);
    expect(Object.keys(o?.mcpServers ?? {})).toEqual(["coach"]);
    expect(o?.includePartialMessages).toBe(true);
  });

  it("a_turn_without_tools_mounts_no_coach_server", async () => {
    const s = useScript([textDelta("plain"), result()]);
    const res = new FakeRes();
    const body = { ...baseBody, tools: [], toolCallback: undefined };
    await runChat(parseChatRequest(body), testConfig(), res, deps());
    expect(terminal(res).map((e) => e.event)).toEqual(["done"]);
    const o = s.calls[0]?.options;
    expect(o?.mcpServers).toBeUndefined();
    expect(o?.allowedTools).toBeUndefined();
  });

  it("stream_ends_with_exactly_one_terminal_event", async () => {
    const scenarios: Parameters<typeof scripted>[0] = [
      [textDelta("a"), result()],
      [result({ subtype: "error_max_turns", is_error: true })],
      [result({ is_error: true, api_error_status: 429 })],
      [textDelta("a")], // the iterator ends with no result
      async function* () {
        yield textDelta("a");
        throw new Error("boom");
      },
      async function* () {
        yield result();
        throw new Error("exit code 1 after result");
      },
    ];
    for (const sc of scenarios) {
      useScript(sc);
      const res = new FakeRes();
      await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
      expect(terminal(res)).toHaveLength(1);
      expect(res.writableEnded).toBe(true);
    }
    useScript([result({ is_error: true, api_error_status: 429 })]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(terminal(res)[0]).toEqual({
      event: "error",
      data: {
        type: "error",
        error: { type: "rate_limit_error", message: expect.any(String) },
        status: 429,
      },
    });
  });

  it("sdk_throw_after_headers_is_an_error_event", async () => {
    useScript(async function* () {
      yield textDelta("partial");
      throw new Error("Claude Code crashed: sk-ant-test-SECRET123");
    });
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    const ev = res.events();
    expect(ev.map((e) => e.event)).toEqual(["delta", "error"]);
    expect(ev[1]?.data).toEqual({
      type: "error",
      error: { type: "api_error", message: expect.any(String) },
      status: 500,
    });
    expect(res.body()).not.toContain("SECRET");
  });

  it("timeout_is_an_error_event_504", async () => {
    useScript(async function* (signal) {
      yield textDelta("a");
      await new Promise((r) => signal.addEventListener("abort", r));
      throw new Error("aborted");
    });
    const res = new FakeRes();
    await runChat(parseChatRequest({ ...baseBody, timeoutMs: 30 }), testConfig(), res, deps());
    expect(terminal(res)).toEqual([
      { event: "error", data: expect.objectContaining({ status: 504 }) },
    ]);
  });

  it("client_disconnect_aborts_the_query", async () => {
    let aborted = false;
    const res = new FakeRes();
    useScript(async function* (signal) {
      yield textDelta("a");
      res.disconnect();
      await new Promise<void>((r) => {
        if (signal.aborted) r();
        signal.addEventListener("abort", () => r());
      });
      aborted = signal.aborted;
      throw new Error("aborted");
    });
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(aborted).toBe(true);
  });

  it("pings_every_15s", async () => {
    vi.useFakeTimers();
    let release!: () => void;
    useScript(async function* () {
      await new Promise<void>((r) => {
        release = r;
      });
      yield result();
    });
    const res = new FakeRes();
    const p = runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    await vi.advanceTimersByTimeAsync(14999);
    expect(res.body()).not.toContain(": ping");
    await vi.advanceTimersByTimeAsync(1);
    expect(res.body().match(/: ping\n\n/g)).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(30000);
    expect(res.body().match(/: ping\n\n/g)).toHaveLength(3);
    release();
    await p;
    const after = res.chunks.length;
    await vi.advanceTimersByTimeAsync(60000);
    expect(res.chunks.length).toBe(after); // the ping stops with the stream
  });

  it("verbatim_prompts_reach_query", async () => {
    const s = useScript([result()]);
    await runChat(parseChatRequest(baseBody), testConfig(), new FakeRes(), deps());
    expect(s.calls[0]?.options.verbatimPrompts).toBe(true);
  });

  it("headers_are_flushed_immediately", async () => {
    useScript([result()]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(res.flushed).toBe(1);
  });

  // ---- sessions, against a fake CLI that enforces the SDK's session rules ----

  function cliEnv() {
    const cfg = testConfig({ AI_SIDECAR_CONFIG_DIR: tempDir() });
    const cli = new FakeCli();
    vi.mocked(sdk.query).mockImplementation(cli.fn as never);
    return { cfg, cli, d: deps() };
  }
  const localFile = (cfg: { configDir: string }) =>
    join(cfg.configDir, "projects", PROJECT_DIR_NAME, `${CONV}.jsonl`);

  it("new_conversation_runs_with_session_id_and_the_store", async () => {
    const { cfg, cli, d } = cliEnv();
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), cfg, res, d);
    const o = cli.calls[0]?.options;
    expect(o?.sessionId).toBe(CONV);
    expect(o?.sessionStore).toBe(d.store);
    expect(o).not.toHaveProperty("resume");
    expect(o).not.toHaveProperty("persistSession");
    const [msg] = await promptMessages(cli.calls[0]?.prompt);
    expect(msg?.message.content).toEqual(baseBody.prompt);
    expect(terminal(res)[0]?.data).toMatchObject({ subtype: "success", session: "new" });
    expect(d.store.rows.get(CONV)).toHaveLength(2);
  });

  it("with_a_store_no_local_transcript_survives_the_turn", async () => {
    const { cfg, d } = cliEnv();
    await runChat(parseChatRequest(baseBody), cfg, new FakeRes(), d);
    expect(existsSync(localFile(cfg))).toBe(false);
    expect(existsSync(join(cfg.configDir, "projects", PROJECT_DIR_NAME, CONV))).toBe(false);
  });

  it("next_turn_resumes_with_only_the_new_message", async () => {
    const { cfg, cli, d } = cliEnv();
    await runChat(parseChatRequest(baseBody), cfg, new FakeRes(), d);
    const res = new FakeRes();
    await runChat(
      parseChatRequest({ ...baseBody, resume: true, rebuildContext: "SUMMARY" }),
      cfg,
      res,
      d,
    );
    expect(cli.calls).toHaveLength(2);
    const o = cli.calls[1]?.options;
    expect(o?.resume).toBe(CONV);
    expect(o).not.toHaveProperty("sessionId");
    const msgs = await promptMessages(cli.calls[1]?.prompt);
    expect(msgs).toHaveLength(1);
    expect(msgs[0]?.message.content).toEqual(baseBody.prompt); // no rebuild context, no history
    expect(terminal(res)[0]?.data).toMatchObject({ subtype: "success", session: "resumed" });
    expect(cli.scratchDirs).toHaveLength(1); // served from the store via a temporary dir
  });

  it("resumed_turn_reports_per_turn_usage_and_session_totals", async () => {
    const { cfg, d } = cliEnv();
    const first = new FakeRes();
    await runChat(parseChatRequest(baseBody), cfg, first, d);
    const turn = {
      inputTokens: 10,
      outputTokens: 20,
      cacheReadInputTokens: 100,
      cacheCreationInputTokens: 5,
    };
    expect(terminal(first)[0]?.data).toMatchObject({
      modelUsage: { "claude-sonnet-5-5": turn },
      sessionModelUsage: { "claude-sonnet-5-5": turn },
    });
    const second = new FakeRes();
    await runChat(
      parseChatRequest({ ...baseBody, resume: true, rebuildContext: "S" }),
      cfg,
      second,
      d,
    );
    // The SDK's modelUsage on a resumed turn includes the earlier turn; Rust meters modelUsage.
    expect(terminal(second)[0]?.data).toMatchObject({
      session: "resumed",
      modelUsage: { "claude-sonnet-5-5": turn },
      sessionModelUsage: {
        "claude-sonnet-5-5": {
          inputTokens: 20,
          outputTokens: 40,
          cacheReadInputTokens: 200,
          cacheCreationInputTokens: 10,
        },
      },
    });
  });

  it("failed_resume_removes_the_stale_local_transcript_and_rebuilds", async () => {
    const { cfg, cli, d } = cliEnv();
    // No store copy (sessions predate the store, or a mirror outage) but a stale local file, and
    // the CLI cannot resume it. Without removing it, the rebuild's sessionId would be "in use".
    mkdirSync(dirname(localFile(cfg)), { recursive: true });
    writeFileSync(localFile(cfg), "{corrupt\n");
    mkdirSync(join(cfg.configDir, "projects", PROJECT_DIR_NAME, CONV, "subagents"), {
      recursive: true,
    });
    cli.corruptResume = true;
    const res = new FakeRes();
    await runChat(
      parseChatRequest({ ...baseBody, resume: true, rebuildContext: "<history>SUMMARY</history>" }),
      cfg,
      res,
      d,
    );
    expect(cli.calls).toHaveLength(2);
    expect(cli.calls[0]?.options.resume).toBe(CONV);
    expect(cli.calls[1]?.options.sessionId).toBe(CONV);
    expect(cli.calls[1]?.options).not.toHaveProperty("resume");
    expect(d.store.deleted).toEqual([CONV]);
    const msgs = await promptMessages(cli.calls[1]?.prompt);
    expect(msgs[0]?.message.content).toEqual([
      { type: "text", text: "<history>SUMMARY</history>" },
      ...baseBody.prompt,
    ]);
    expect(res.events().map((e) => e.event)).toEqual(["delta", "done"]);
    expect(terminal(res)[0]?.data).toMatchObject({ subtype: "success", session: "rebuilt" });
  });

  it("new_turn_clears_a_leftover_local_transcript_first", async () => {
    const { cfg, d } = cliEnv();
    mkdirSync(dirname(localFile(cfg)), { recursive: true });
    writeFileSync(localFile(cfg), "{}\n");
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), cfg, res, d);
    expect(terminal(res)[0]?.data).toMatchObject({ subtype: "success", session: "new" });
  });

  it("missing_transcript_goes_straight_to_rebuilt", async () => {
    const { cfg, cli, d } = cliEnv(); // empty store, no local file
    const res = new FakeRes();
    await runChat(
      parseChatRequest({ ...baseBody, resume: true, rebuildContext: "CTX" }),
      cfg,
      res,
      d,
    );
    expect(cli.calls).toHaveLength(1);
    expect(cli.calls[0]?.options.sessionId).toBe(CONV);
    expect(terminal(res)[0]?.data).toMatchObject({ session: "rebuilt" });
  });

  it("no_retry_after_output_was_streamed", async () => {
    const s = useScript([
      textDelta("half"),
      result({ subtype: "error_during_execution", is_error: true }),
    ]);
    const d = deps();
    d.store.rows.set(CONV, [{ type: "user" }]);
    const res = new FakeRes();
    await runChat(
      parseChatRequest({ ...baseBody, resume: true, rebuildContext: "S" }),
      testConfig(),
      res,
      d,
    );
    expect(s.calls).toHaveLength(1);
    expect(terminal(res)[0]?.data).toMatchObject({
      subtype: "error_during_execution",
      session: "resumed",
    });
  });

  it("shutdown_ends_the_stream_with_a_503_error_event", async () => {
    const shutdown = new AbortController();
    useScript(async function* (signal) {
      yield textDelta("a");
      shutdown.abort();
      await new Promise((r) => {
        if (signal.aborted) r(undefined);
        signal.addEventListener("abort", r);
      });
      throw new Error("aborted");
    });
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, {
      ...deps(),
      shutdown: shutdown.signal,
    });
    expect(terminal(res)).toEqual([
      { event: "error", data: expect.objectContaining({ status: 503 }) },
    ]);
  });

  const start = (id: string, model = "claude-sonnet-5-5") =>
    ({
      type: "stream_event",
      event: { type: "message_start", message: { id, model, usage: {} } },
      parent_tool_use_id: null,
      uuid: "u",
      session_id: "s",
    }) as unknown as SDKMessage;
  const stop = (reason: string) =>
    ({
      type: "stream_event",
      event: { type: "message_delta", delta: { stop_reason: reason }, usage: { output_tokens: 3 } },
      parent_tool_use_id: null,
      uuid: "u",
      session_id: "s",
    }) as unknown as SDKMessage;
  const assistant = (uuid: string, id: string, text: string, supersedes?: string[]) =>
    ({
      type: "assistant",
      message: { id, content: [{ type: "text", text }] },
      parent_tool_use_id: null,
      ...(supersedes ? { supersedes } : {}),
      uuid,
      session_id: "s",
    }) as unknown as SDKMessage;

  it("refusal_fallback_retracts_only_the_refused_streamed_message", async () => {
    useScript([
      start("msg_1"),
      textDelta("refused text"),
      stop("refusal"),
      assistant("a-1", "msg_1", "refused text"),
      start("msg_2", "claude-fallback-x"),
      textDelta("fallback text"),
      stop("end_turn"),
      // The replacement names the refused message (already retracted) and one never seen.
      assistant("a-2", "msg_2", "fallback text", ["a-1", "a-0"]),
      {
        type: "system",
        subtype: "model_refusal_fallback",
        trigger: "refusal",
        direction: "retry",
        original_model: "claude-sonnet-5-5",
        fallback_model: "claude-fallback-x",
        request_id: null,
        retracted_message_uuids: ["a-1", "never-seen"],
        content: "SECRET refusal text",
        uuid: "u",
        session_id: "s",
      } as unknown as SDKMessage,
      result({
        result: "fallback text",
        modelUsage: {
          "claude-sonnet-5-5": usage(1, 2, 0, 0),
          "claude-fallback-x": usage(3, 4, 0, 0),
        },
      }),
    ]);
    const d = deps();
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, d);
    // The retract comes before any replacement text, names only what was streamed, and the
    // later notices (already retracted, or unknown uuids) are no-ops.
    expect(res.events()).toEqual([
      { event: "delta", data: { text: "refused text", messageId: "msg_1" } },
      { event: "retract", data: { messageIds: ["msg_1"], reason: "refusal" } },
      { event: "delta", data: { text: "fallback text", messageId: "msg_2" } },
      { event: "done", data: expect.objectContaining({ text: "fallback text" }) },
    ]);
    // Unknown model keys pass through unchanged for Rust to price conservatively.
    const done = terminal(res)[0]?.data as { modelUsage: object } | undefined;
    expect(Object.keys(done?.modelUsage ?? {})).toEqual(["claude-sonnet-5-5", "claude-fallback-x"]);
    const all = d.logger.lines.join("\n");
    expect(all).toContain("model_refusal_fallback");
    expect(all).not.toContain("SECRET");
  });

  it("superseded_streamed_message_is_retracted_once", async () => {
    useScript([
      start("msg_1"),
      textDelta("bad tool call text"),
      stop("tool_use"),
      assistant("a-1", "msg_1", "bad tool call text"),
      start("msg_2"),
      textDelta("good text"),
      stop("end_turn"),
      assistant("a-2", "msg_2", "good text", ["a-1"]),
      assistant("a-3", "msg_3", "", ["a-1", "a-2-unknown"]),
      result({ result: "good text" }),
    ]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(res.events().filter((e) => e.event === "retract")).toEqual([
      { event: "retract", data: { messageIds: ["msg_1"], reason: "superseded" } },
    ]);
    expect(terminal(res)[0]?.data).toMatchObject({ text: "good text" });
  });

  it("a_superseded_tool_call_message_is_retracted", async () => {
    useScript([
      start("msg_1"),
      stop("tool_use"),
      {
        type: "assistant",
        message: {
          id: "msg_tool",
          content: [
            { type: "tool_use", id: "toolu_bad", name: "mcp__coach__get_position", input: {} },
          ],
        },
        parent_tool_use_id: null,
        uuid: "a-tool",
        session_id: "s",
      } as unknown as SDKMessage,
      start("msg_2"),
      textDelta("fixed"),
      stop("end_turn"),
      assistant("a-2", "msg_2", "fixed", ["a-tool"]),
      result({ result: "fixed" }),
    ]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, deps());
    expect(res.events().map((e) => e.event)).toEqual(["tool_use", "delta", "retract", "done"]);
    expect(res.events()[2]?.data).toEqual({ messageIds: ["msg_tool"], reason: "superseded" });
  });

  // ---- per-conversation turn lock ----

  it("a_concurrent_turn_on_the_same_conversation_is_409", async () => {
    let release!: () => void;
    useScript(async function* () {
      await new Promise<void>((r) => {
        release = r;
      });
      yield result();
    });
    const d = deps();
    const first = new FakeRes();
    const p1 = runChat(parseChatRequest(baseBody), testConfig(), first, d);
    await vi.waitFor(() => expect(first.headersSent).toBe(true));
    const second = new FakeRes();
    const e = await runChat(parseChatRequest(baseBody), testConfig(), second, d).catch((x) => x);
    expect(e).toBeInstanceOf(SidecarError);
    expect([e.status, e.type]).toEqual([409, "turn_in_progress"]);
    expect(second.headersSent).toBe(false);
    release();
    await p1;
    expect(first.status).toBe(200);
    expect(terminal(first)[0]?.event).toBe("done");
    // Released: the next turn runs.
    const third = new FakeRes();
    useScript([result()]);
    await runChat(parseChatRequest(baseBody), testConfig(), third, d);
    expect(terminal(third)[0]?.event).toBe("done");
  });

  it("a_turn_running_on_another_replica_is_409", async () => {
    const s = useScript([result()]);
    const d = deps();
    d.store.held.add(CONV); // another replica holds the Postgres advisory lock
    const e = await runChat(parseChatRequest(baseBody), testConfig(), new FakeRes(), d).catch(
      (x) => x,
    );
    expect([e.status, e.type]).toEqual([409, "turn_in_progress"]);
    expect(s.calls).toHaveLength(0);
    // The in-process lock was released despite the 409.
    d.store.held.delete(CONV);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, d);
    expect(terminal(res)[0]?.event).toBe("done");
    expect(d.store.held.size).toBe(0); // the store lock is released after the turn
  });

  it("both_locks_are_free_after_every_way_a_turn_can_end", async () => {
    const hang = (onStart: (signal: AbortSignal) => void) =>
      async function* (signal: AbortSignal): AsyncGenerator<SDKMessage> {
        onStart(signal);
        yield textDelta("a");
        await new Promise((r) => {
          if (signal.aborted) r(undefined);
          signal.addEventListener("abort", r);
        });
        throw new Error("aborted");
      };
    type Case = { name: string; run: (d: ReturnType<typeof deps>) => Promise<unknown> };
    const cases: Case[] = [
      {
        name: "sdk error",
        run: (d) => {
          useScript(async function* () {
            yield textDelta("a");
            throw new Error("boom");
          });
          return runChat(parseChatRequest(baseBody), testConfig(), new FakeRes(), d);
        },
      },
      {
        name: "timeout abort",
        run: (d) => {
          useScript(hang(() => {}));
          const req = parseChatRequest({ ...baseBody, timeoutMs: 20 });
          return runChat(req, testConfig(), new FakeRes(), d);
        },
      },
      {
        name: "client disconnect",
        run: (d) => {
          const res = new FakeRes();
          useScript(hang(() => res.disconnect()));
          return runChat(parseChatRequest(baseBody), testConfig(), res, d);
        },
      },
      {
        name: "SIGTERM",
        run: (d) => {
          const shutdown = new AbortController();
          useScript(hang(() => shutdown.abort()));
          return runChat(parseChatRequest(baseBody), testConfig(), new FakeRes(), {
            ...d,
            shutdown: shutdown.signal,
          });
        },
      },
      {
        name: "tool schema 400 never takes a lock",
        run: (d) =>
          runChat(
            parseChatRequest({
              ...baseBody,
              tools: [{ name: "x", description: "d", inputSchema: { type: "object", oneOf: [] } }],
            }),
            testConfig(),
            new FakeRes(),
            d,
          ).catch(() => {}),
      },
    ];
    for (const c of cases) {
      const d = deps();
      await c.run(d);
      expect([c.name, d.turns.size, d.store.held.size]).toEqual([c.name, 0, 0]);
      useScript([result()]);
      const res = new FakeRes();
      await runChat(parseChatRequest(baseBody), testConfig(), res, d);
      expect([c.name, res.status, terminal(res)[0]?.event]).toEqual([c.name, 200, "done"]);
    }
  });

  it("other_conversations_are_not_blocked", async () => {
    const releases: Array<() => void> = [];
    useScript(async function* () {
      await new Promise<void>((r) => {
        releases.push(r);
      });
      yield result();
    });
    const d = deps();
    const p1 = runChat(parseChatRequest(baseBody), testConfig(), new FakeRes(), d);
    await vi.waitFor(() => expect(d.store.held.size).toBe(1));
    const other = new FakeRes();
    const p2 = runChat(
      parseChatRequest({ ...baseBody, conversationId: "0b8f2a52-3f1d-4c7e-9e3a-2f6a1c0d9e11" }),
      testConfig(),
      other,
      d,
    );
    await vi.waitFor(() => expect(releases).toHaveLength(2));
    expect(other.headersSent).toBe(true);
    for (const r of releases) r();
    await Promise.all([p1, p2]);
  });

  it("a_retried_first_turn_leaves_one_clean_transcript", async () => {
    const { cfg, cli, d } = cliEnv();
    // A first turn that wrote to the store and then failed (e.g. a timeout) before Rust saw done.
    d.store.rows.set(CONV, [
      { type: "user", uuid: "stale-1" },
      { type: "assistant" },
      { type: "x" },
    ]);
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), cfg, res, d);
    expect(terminal(res)[0]?.data).toMatchObject({ subtype: "success", session: "new" });
    expect(d.store.deleted).toEqual([CONV]);
    const rows = d.store.rows.get(CONV) as Array<{ uuid?: string }>;
    expect(rows).toHaveLength(2); // only the retried turn's entries
    expect(rows.some((r) => r.uuid === "stale-1")).toBe(false);
    expect(cli.calls[0]?.options.sessionId).toBe(CONV);
  });

  it("a_failed_resume_attempt_usage_is_added_to_the_rebuilt_turn", async () => {
    useScript(
      [
        start("msg_r"),
        {
          type: "stream_event",
          event: {
            type: "message_delta",
            delta: {},
            usage: { input_tokens: 7, output_tokens: 0, cache_read_input_tokens: 0 },
          },
          parent_tool_use_id: null,
          uuid: "u",
          session_id: "s",
        } as unknown as SDKMessage,
        result({ subtype: "error_during_execution", is_error: true, modelUsage: {} }),
      ],
      [result({ modelUsage: { "claude-sonnet-5-5": usage(10, 20, 100, 5) } })],
    );
    const d = deps();
    d.store.rows.set(CONV, [{ type: "user" }]);
    const res = new FakeRes();
    await runChat(
      parseChatRequest({ ...baseBody, resume: true, rebuildContext: "CTX" }),
      testConfig(),
      res,
      d,
    );
    expect(terminal(res)[0]?.data).toMatchObject({
      session: "rebuilt",
      modelUsage: {
        "claude-sonnet-5-5": {
          inputTokens: 17,
          outputTokens: 20,
          cacheReadInputTokens: 100,
          cacheCreationInputTokens: 5,
        },
      },
    });
  });

  it("mirror_error_is_a_warning_metric", async () => {
    useScript([
      {
        type: "system",
        subtype: "mirror_error",
        error: "connection refused postgres://ai_sidecar:pw@db",
        uuid: "u",
        session_id: "s",
      } as unknown as SDKMessage,
      result(),
    ]);
    const d = deps();
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, d);
    expect(d.metrics.snapshot().mirrorErrors).toBe(1);
    const warn = d.logger.lines.find((l) => l.includes("mirror_error"));
    expect(warn).toBeDefined();
    expect(warn).not.toContain("pw@");
    expect(terminal(res)[0]?.event).toBe("done");
  });
});
