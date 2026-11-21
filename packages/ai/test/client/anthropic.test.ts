import { describe, expect, it } from "vitest";
import { z } from "zod";
import { createAnthropicAiClient } from "../../src/client/anthropic.js";
import { AiRequestError, AiUnavailableError } from "../../src/client/errors.js";
import type { StructuredRequest } from "../../src/client/types.js";

const KEY = "sk-ant-test-SECRET123";
const schema = z.object({ section: z.string(), n: z.number() });

function req(): StructuredRequest {
  return {
    model: "claude-sonnet-5-5",
    system: [{ type: "text", text: "sys", cache_control: { type: "ephemeral" } }],
    messages: [{ role: "user", content: "hi" }],
    maxTokens: 1000,
    effort: "medium",
    schema,
  };
}

function okBody(text: string, stop = "end_turn") {
  return {
    id: "msg_1",
    type: "message",
    role: "assistant",
    model: "claude-sonnet-5-5",
    content: [{ type: "text", text }],
    stop_reason: stop,
    stop_sequence: null,
    usage: {
      input_tokens: 10,
      output_tokens: 5,
      cache_creation_input_tokens: null,
      cache_read_input_tokens: 7,
    },
  };
}

interface Body {
  model: string;
  output_config: { effort: string; format: { type: string } };
  thinking?: { budget_tokens?: number };
  requests: { params: Body }[];
}
type Captured = { body?: Body };

function stub(status: number, body: unknown, capture?: Captured): typeof fetch {
  return (async (_u: unknown, init?: RequestInit) => {
    if (capture) capture.body = JSON.parse(String(init?.body));
    return new Response(JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });
  }) as typeof fetch;
}

const mk = (f: typeof fetch) => createAnthropicAiClient({ apiKey: KEY, fetch: f, maxRetries: 0 });

describe("anthropic adapter structured()", () => {
  it("sends output_config and no sampling params; maps usage", async () => {
    const cap: Captured = {};
    const ai = mk(stub(200, okBody('{"section":"a","n":3}'), cap));
    const r = await ai.structured(req());
    expect(r.parsed).toEqual({ section: "a", n: 3 });
    expect(r.usage).toEqual({
      inputTokens: 10,
      outputTokens: 5,
      cacheCreationTokens: 0,
      cacheReadTokens: 7,
    });
    const b = cap.body as Body;
    expect(b.model).toBe("claude-sonnet-5-5");
    expect(b.output_config.effort).toBe("medium");
    expect(b.output_config.format.type).toBe("json_schema");
    for (const k of ["temperature", "top_p", "top_k", "tool_choice"]) {
      expect(b).not.toHaveProperty(k);
    }
    expect(b.thinking?.budget_tokens).toBeUndefined();
  });

  it("refusal gives parsed null", async () => {
    const ai = mk(stub(200, okBody("Declined.", "refusal")));
    const r = await ai.structured(req());
    expect(r.parsed).toBeNull();
    expect(r.stopReason).toBe("refusal");
  });

  it("max_tokens truncation gives parsed null instead of throwing", async () => {
    const ai = mk(stub(200, okBody('{"section":"a","n', "max_tokens")));
    const r = await ai.structured(req());
    expect(r.parsed).toBeNull();
    expect(r.stopReason).toBe("max_tokens");
  });

  it("maps 500, 529, 429 and connection errors", async () => {
    const e500 = await mk(stub(500, { type: "error", error: { type: "api_error", message: "x" } }))
      .structured(req())
      .catch((e) => e);
    expect(e500).toBeInstanceOf(AiUnavailableError);
    expect(e500.reason).toBe("server");
    const e529 = await mk(
      stub(529, { type: "error", error: { type: "overloaded_error", message: "x" } }),
    )
      .structured(req())
      .catch((e) => e);
    expect(e529.reason).toBe("overloaded");
    const e429 = await mk(
      stub(429, { type: "error", error: { type: "rate_limit_error", message: "x" } }),
    )
      .structured(req())
      .catch((e) => e);
    expect(e429.reason).toBe("rate_limited");
    const reset = (async () => {
      throw new TypeError("fetch failed");
    }) as unknown as typeof fetch;
    const eConn = await mk(reset)
      .structured(req())
      .catch((e) => e);
    expect(eConn).toBeInstanceOf(AiUnavailableError);
    expect(eConn.reason).toBe("connection");
  });

  it("secrets never leak through errors", async () => {
    const ai = mk(
      stub(401, {
        type: "error",
        error: { type: "authentication_error", message: `invalid x-api-key ${KEY}` },
      }),
    );
    const err = await ai.structured(req()).catch((e) => e);
    expect(err).toBeInstanceOf(AiRequestError);
    expect(err.status).toBe(401);
    for (const s of [err.message, String(err), JSON.stringify(err), err.stack ?? ""]) {
      expect(s).not.toContain("SECRET123");
    }
  });
});

describe("anthropic adapter batch", () => {
  it("submits with format and maps results via the schema registry", async () => {
    const cap: Captured = {};
    const lines = [
      { custom_id: "a", result: { type: "succeeded", message: okBody('{"section":"s","n":1}') } },
      { custom_id: "b", result: { type: "succeeded", message: okBody("not json") } },
      {
        custom_id: "c",
        result: {
          type: "errored",
          error: { type: "error", error: { type: "overloaded_error", message: "x" } },
        },
      },
      { custom_id: "d", result: { type: "expired" } },
    ];
    const json = { "content-type": "application/json" };
    const f = (async (u: unknown, init?: RequestInit) => {
      if (String(u).endsWith("/results")) {
        return new Response(lines.map((l) => JSON.stringify(l)).join("\n"), {
          status: 200,
          headers: { "content-type": "application/x-jsonl" },
        });
      }
      if (init?.method === "POST") {
        cap.body = JSON.parse(String(init.body));
        return new Response(
          JSON.stringify({
            id: "batch_1",
            processing_status: "in_progress",
            type: "message_batch",
          }),
          { status: 200, headers: json },
        );
      }
      return new Response(
        JSON.stringify({
          id: "batch_1",
          processing_status: "ended",
          results_url: "https://api.anthropic.com/v1/messages/batches/batch_1/results",
        }),
        { status: 200, headers: json },
      );
    }) as typeof fetch;
    const ai = mk(f);
    const id = await ai.batch.submit(
      ["a", "b", "c", "d"].map((customId) => ({ customId, req: req() })),
    );
    expect(id).toBe("batch_1");
    expect(cap.body?.requests).toHaveLength(4);
    expect(cap.body?.requests[0]?.params.output_config.format.type).toBe("json_schema");
    expect(await ai.batch.status(id)).toBe("ended");
    const out = [];
    for await (const r of ai.batch.results(id)) out.push(r);
    expect(out.map((o) => [o.customId, o.outcome])).toEqual([
      ["a", "succeeded"],
      ["b", "succeeded"],
      ["c", "errored"],
      ["d", "expired"],
    ]);
    expect(out[0]?.result?.parsed).toEqual({ section: "s", n: 1 });
    expect(out[1]?.result?.parsed).toBeNull();
    expect(out[2]?.errorType).toBe("overloaded_error");
  });
});
