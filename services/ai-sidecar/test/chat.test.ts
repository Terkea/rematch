import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { SDKMessage, SDKUserMessage } from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type ChatSessionStore, runChat } from "../src/chat.js";
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
}

function deps() {
  return {
    limiter: new Limiter(4, 32, 30000),
    logger: captureLogger(),
    metrics: new Metrics(),
    store: new FakeStore(),
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

  it("refusal_fallback_retracts_the_refused_message_and_is_logged", async () => {
    const start = (id: string) =>
      ({
        type: "stream_event",
        event: { type: "message_start", message: { id, model: "claude-sonnet-5-5", usage: {} } },
        parent_tool_use_id: null,
        uuid: "u",
        session_id: "s",
      }) as unknown as SDKMessage;
    useScript([
      start("msg_1"),
      textDelta("refused text"),
      {
        type: "assistant",
        message: { id: "msg_1", content: [{ type: "text", text: "refused text" }] },
        parent_tool_use_id: null,
        uuid: "a-1",
        session_id: "s",
      } as unknown as SDKMessage,
      {
        type: "system",
        subtype: "model_refusal_fallback",
        trigger: "refusal",
        direction: "retry",
        original_model: "claude-sonnet-5-5",
        fallback_model: "claude-fallback-x",
        request_id: null,
        retracted_message_uuids: ["a-1"],
        content: "SECRET refusal text",
        uuid: "u",
        session_id: "s",
      } as unknown as SDKMessage,
      start("msg_2"),
      textDelta("fallback text"),
      {
        type: "assistant",
        message: { id: "msg_2", content: [{ type: "text", text: "fallback text" }] },
        parent_tool_use_id: null,
        supersedes: ["a-0"],
        uuid: "a-2",
        session_id: "s",
      } as unknown as SDKMessage,
      result({
        modelUsage: {
          "claude-sonnet-5-5": usage(1, 2, 0, 0),
          "claude-fallback-x": usage(3, 4, 0, 0),
        },
      }),
    ]);
    const d = deps();
    const res = new FakeRes();
    await runChat(parseChatRequest(baseBody), testConfig(), res, d);
    const ev = res.events();
    expect(ev.slice(0, 4)).toEqual([
      { event: "delta", data: { text: "refused text", messageId: "msg_1" } },
      { event: "retract", data: { messageIds: ["msg_1"], reason: "model_refusal_fallback" } },
      { event: "delta", data: { text: "fallback text", messageId: "msg_2" } },
      // a-0 was never seen, so its text cannot be named: retract everything of this turn so far.
      { event: "retract", data: { messageIds: [], all: true, reason: "superseded" } },
    ]);
    // Unknown model keys pass through unchanged for Rust to price conservatively.
    const done = terminal(res)[0]?.data as { modelUsage: object } | undefined;
    expect(Object.keys(done?.modelUsage ?? {})).toEqual(["claude-sonnet-5-5", "claude-fallback-x"]);
    const all = d.logger.lines.join("\n");
    expect(all).toContain("model_refusal_fallback");
    expect(all).not.toContain("SECRET");
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
