import type { SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SidecarError } from "../src/errors.js";
import { Limiter } from "../src/limiter.js";
import { parseQueryRequest } from "../src/options.js";
import { callController, runQuery } from "../src/query.js";
import { captureLogger, result, scripted, testConfig, usage } from "./helpers.js";

vi.mock("@anthropic-ai/claude-agent-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@anthropic-ai/claude-agent-sdk")>()),
  query: vi.fn(),
}));

const req = parseQueryRequest({
  model: "claude-sonnet-5-5",
  effort: "medium",
  maxTokens: 16000,
  timeoutMs: 600000,
  maxTurns: 3,
  systemPrompt: "SYS TOPSECRETPROMPT",
  prompt: [
    { type: "text", text: "<coaching_input>TOPSECRETPROMPT</coaching_input>" },
    { type: "text", text: "section task" },
  ],
  outputFormat: { type: "json_schema", schema: { type: "object" } },
});

function useScript(...scripts: Parameters<typeof scripted>[0]) {
  const s = scripted(scripts);
  vi.mocked(sdk.query).mockImplementation(s.fn as never);
  return s;
}

const deps = () => ({ limiter: new Limiter(4, 32, 30000), logger: captureLogger() });

describe("runQuery", () => {
  beforeEach(() => {
    vi.mocked(sdk.query).mockReset();
  });

  it("returns_structured_output_and_model_usage", async () => {
    const s = useScript([
      result({
        structured_output: { headline: "h" },
        result: '{"headline":"h"}',
        modelUsage: {
          "claude-sonnet-5-5": usage(12, 800, 5100, 0),
          "claude-haiku-5-5": usage(3, 4, 5, 6),
        },
        total_cost_usd: 0.0456,
      }),
    ]);
    const r = await runQuery(req, testConfig(), deps());
    expect(r).toEqual({
      subtype: "success",
      stopReason: "end_turn",
      structuredOutput: { headline: "h" },
      text: '{"headline":"h"}',
      numTurns: 1,
      durationMs: 4210,
      modelUsage: {
        "claude-sonnet-5-5": {
          inputTokens: 12,
          outputTokens: 800,
          cacheReadInputTokens: 5100,
          cacheCreationInputTokens: 0,
        },
        "claude-haiku-5-5": {
          inputTokens: 3,
          outputTokens: 4,
          cacheReadInputTokens: 5,
          cacheCreationInputTokens: 6,
        },
      },
      sdkCostUsd: 0.0456,
    });
    // The prompt goes in as one user message with the request's text blocks.
    expect(s.calls).toHaveLength(1);
    const it0 = s.calls[0]?.prompt as AsyncIterable<SDKMessage>;
    const msgs: unknown[] = [];
    for await (const m of it0) msgs.push(m);
    expect(msgs).toEqual([
      expect.objectContaining({
        type: "user",
        parent_tool_use_id: null,
        message: { role: "user", content: req.prompt },
      }),
    ]);
    expect(s.calls[0]?.options.persistSession).toBe(false);
  });

  it("max_structured_output_retries_is_a_200_with_subtype", async () => {
    useScript([
      result({
        subtype: "error_max_structured_output_retries",
        is_error: true,
        result: undefined,
        errors: ["x"],
      }),
    ]);
    const r = await runQuery(req, testConfig(), deps());
    expect(r.subtype).toBe("error_max_structured_output_retries");
    expect(r).not.toHaveProperty("structuredOutput");
    expect(r.text).toBe("");
  });

  it("refusal_stop_reason_passes_through", async () => {
    useScript([result({ stop_reason: "refusal", result: "" })]);
    const r = await runQuery(req, testConfig(), deps());
    expect(r.stopReason).toBe("refusal");
  });

  it("a_throw_after_the_result_keeps_the_result", async () => {
    useScript(async function* () {
      yield result({ subtype: "error_max_turns", is_error: true });
      throw new Error("Claude Code process exited with code 1");
    });
    const r = await runQuery(req, testConfig(), deps());
    expect(r.subtype).toBe("error_max_turns");
  });

  it("api_error_status_maps_to_anthropic_error_body", async () => {
    const cases: Array<[number, string, string | undefined]> = [
      [400, "invalid_request_error", undefined],
      [401, "authentication_error", undefined],
      [403, "permission_error", undefined],
      [429, "rate_limit_error", "7"],
      [529, "overloaded_error", undefined],
      [500, "api_error", undefined],
      [503, "api_error", undefined],
    ];
    for (const [status, type, retryAfter] of cases) {
      const msgs: SDKMessage[] = [];
      if (status === 429) {
        msgs.push({
          type: "system",
          subtype: "api_retry",
          attempt: 1,
          max_retries: 1,
          retry_delay_ms: 6500,
          error_status: 429,
          error: "rate_limit",
          uuid: "u",
          session_id: "s",
        } as unknown as SDKMessage);
      }
      msgs.push(
        result({
          is_error: true,
          api_error_status: status,
          result: "API Error: sk-ant-test-SECRET123",
        }),
      );
      useScript(msgs);
      const e = await runQuery(req, testConfig(), deps()).catch((x: unknown) => x);
      expect(e).toBeInstanceOf(SidecarError);
      const err = e as SidecarError;
      expect([err.status, err.type, err.retryAfter]).toEqual([status, type, retryAfter]);
      expect(err.message).not.toContain("SECRET");
    }
  });

  it("timeout_aborts_and_returns_504", async () => {
    let aborted = false;
    useScript(async function* (signal) {
      await new Promise<void>((resolve) => {
        signal.addEventListener("abort", () => {
          aborted = true;
          resolve();
        });
      });
      throw Object.assign(new Error("aborted"), { name: "AbortError" });
    });
    const e = await runQuery({ ...req, timeoutMs: 30 }, testConfig(), deps()).catch((x) => x);
    expect(aborted).toBe(true);
    expect(e).toBeInstanceOf(SidecarError);
    expect([(e as SidecarError).status, (e as SidecarError).type]).toEqual([504, "api_error"]);
  });

  it("sdk_throw_without_result_is_500_api_error", async () => {
    useScript(async function* () {
      yield* [];
      throw new Error("spawn claude ENOENT TOPSECRETPROMPT");
    });
    const e = (await runQuery(req, testConfig(), deps()).catch((x) => x)) as SidecarError;
    expect([e.status, e.type]).toEqual([500, "api_error"]);
    expect(e.message).not.toContain("TOPSECRETPROMPT");
  });

  it("logs_never_contain_prompt_or_secrets", async () => {
    useScript([
      result({ result: "OUTPUT TOPSECRETPROMPT", structured_output: { x: "TOPSECRETPROMPT" } }),
    ]);
    const d = deps();
    await runQuery(req, testConfig(), d);
    useScript([
      result({ is_error: true, api_error_status: 401, result: "bad key sk-ant-test-SECRET123" }),
    ]);
    await runQuery(req, testConfig(), d).catch(() => {});
    const all = d.logger.lines.join("\n");
    expect(d.logger.lines.length).toBeGreaterThanOrEqual(2);
    expect(all).toContain('"route":"/v1/query"');
    expect(all).toContain('"subtype":"success"');
    for (const secret of [
      "TOPSECRETPROMPT",
      "sk-ant-test-SECRET123",
      "SYS",
      "test-sidecar-token",
    ]) {
      expect(all).not.toContain(secret);
    }
  });

  it("verbatim_prompts_reach_query", async () => {
    const s = useScript([result()]);
    await runQuery(req, testConfig(), deps());
    expect(s.calls[0]?.options.verbatimPrompts).toBe(true);
    expect(s.calls[0]?.options.tools).toEqual([]);
    expect(s.calls[0]?.options.permissionMode).toBe("dontAsk");
  });

  it("stateless_query_has_no_session_model_usage", async () => {
    useScript([result()]);
    const r = await runQuery(req, testConfig(), deps());
    expect(r).not.toHaveProperty("sessionModelUsage");
  });

  it("an_already_aborted_parent_aborts_at_once", () => {
    const parent = new AbortController();
    parent.abort();
    const ctl = callController(60000, parent.signal);
    expect(ctl.ac.signal.aborted).toBe(true);
    ctl.dispose();
  });

  it("shutdown_aborts_the_call_with_503", async () => {
    const shutdown = new AbortController();
    useScript(async function* (signal) {
      shutdown.abort();
      await new Promise((r) => {
        if (signal.aborted) r(undefined);
        signal.addEventListener("abort", r);
      });
      throw new Error("aborted");
    });
    const e = (await runQuery(req, testConfig(), { ...deps(), shutdown: shutdown.signal }).catch(
      (x) => x,
    )) as SidecarError;
    expect([e.status, e.type]).toEqual([503, "api_error"]);
  });
});
