import { describe, expect, it } from "vitest";
import { z } from "zod";
import { createAnthropicAiClient } from "../../src/client/anthropic.js";
import { withBreaker } from "../../src/client/breaker.js";
import { decodeCustomId, encodeCustomId } from "../../src/client/custom-id.js";
import {
  AiAuthError,
  AiRequestError,
  AiResponseError,
  AiUnavailableError,
  classifyBatchErrorType,
} from "../../src/client/errors.js";
import { createFakeAiClient } from "../../src/client/fake.js";
import type { BatchResultItem, StructuredRequest } from "../../src/client/types.js";
import { costMicroUsd, UnknownModelError } from "../../src/cost.js";

const KEY = "sk-ant-test-SECRET123";
const schema = z.object({ section: z.string(), n: z.number() });
const json = { "content-type": "application/json" };

function req(): StructuredRequest {
  return {
    model: "claude-sonnet-5-5",
    system: [],
    messages: [{ role: "user", content: "hi" }],
    maxTokens: 100,
    effort: "low",
    schema,
  };
}

function msg(text: string | null, stop = "end_turn") {
  return {
    id: "m",
    type: "message",
    role: "assistant",
    model: "claude-sonnet-5-5",
    content: text === null ? [] : [{ type: "text", text }],
    stop_reason: stop,
    stop_sequence: null,
    usage: { input_tokens: 1, output_tokens: 1 },
  };
}

function respond(
  status: number,
  body: unknown,
  headers: Record<string, string> = {},
): typeof fetch {
  return (async () =>
    new Response(JSON.stringify(body), {
      status,
      headers: { ...json, ...headers },
    })) as typeof fetch;
}

const mk = (f: typeof fetch) => createAnthropicAiClient({ apiKey: KEY, fetch: f, maxRetries: 0 });

describe("parseError reasons (structured)", () => {
  const cases: [string, ReturnType<typeof msg>, string][] = [
    ["no_text", msg(null), "no_text"],
    ["invalid_json", msg("nope"), "invalid_json"],
    ["schema_mismatch", msg('{"section":1}'), "schema_mismatch"],
    ["refusal", msg("x", "refusal"), "refusal"],
    ["truncated", msg('{"sec', "max_tokens"), "truncated"],
  ];
  for (const [name, body, expected] of cases) {
    it(name, async () => {
      const r = await mk(respond(200, body)).structured(req());
      expect(r.parsed).toBeNull();
      expect(r.parseError).toBe(expected);
    });
  }
  it("success has null parseError", async () => {
    const r = await mk(respond(200, msg('{"section":"a","n":1}'))).structured(req());
    expect(r.parseError).toBeNull();
    expect(r.parseIssues).toBeNull();
  });
  it("schema_mismatch lists paths and codes only, no content", async () => {
    const r = await mk(respond(200, msg('{"section":"TOPSECRETVALUE","n":"x"}'))).structured(req());
    expect(r.parseError).toBe("schema_mismatch");
    expect(r.parseIssues).toEqual([{ path: "n", code: "invalid_type" }]);
    expect(JSON.stringify(r.parseIssues)).not.toContain("TOPSECRETVALUE");
  });
});

describe("error taxonomy", () => {
  it("401 and 403 are AiAuthError", async () => {
    for (const status of [401, 403]) {
      const e = await mk(respond(status, { type: "error", error: { type: "x", message: "m" } }))
        .structured(req())
        .catch((x) => x);
      expect(e).toBeInstanceOf(AiAuthError);
      expect(e).toBeInstanceOf(AiRequestError);
      expect(e.status).toBe(status);
    }
  });
  it("400 stays a plain AiRequestError", async () => {
    const e = await mk(respond(400, { type: "error", error: { type: "x", message: "m" } }))
      .structured(req())
      .catch((x) => x);
    expect(e).toBeInstanceOf(AiRequestError);
    expect(e).not.toBeInstanceOf(AiAuthError);
  });
  it("429 exposes retryAfterMs", async () => {
    const f = respond(
      429,
      { type: "error", error: { type: "rate_limit_error", message: "m" } },
      { "retry-after": "7" },
    );
    const e = await mk(f)
      .structured(req())
      .catch((x) => x);
    expect(e.reason).toBe("rate_limited");
    expect(e.retryAfterMs).toBe(7000);
  });
  it("429 without retry-after has undefined retryAfterMs", async () => {
    const f = respond(429, { type: "error", error: { type: "rate_limit_error", message: "m" } });
    const e = await mk(f)
      .structured(req())
      .catch((x) => x);
    expect(e.retryAfterMs).toBeUndefined();
  });
  it("batch error types map to kinds", () => {
    expect(classifyBatchErrorType("overloaded_error")).toBe("retryable");
    expect(classifyBatchErrorType("api_error")).toBe("retryable");
    expect(classifyBatchErrorType("invalid_request_error")).toBe("fatal");
    expect(classifyBatchErrorType("authentication_error")).toBe("fatal");
    expect(classifyBatchErrorType("permission_error")).toBe("fatal");
    expect(classifyBatchErrorType("something_else")).toBe("unknown");
    expect(classifyBatchErrorType(null)).toBe("unknown");
  });
});

describe("breaker and rate limits", () => {
  const exec = () => {
    const failures: string[] = [];
    return {
      failures,
      breaker: {
        exec: async <T>(fn: () => Promise<T>): Promise<T> => {
          try {
            return await fn();
          } catch (e) {
            failures.push((e as AiUnavailableError).reason);
            throw e;
          }
        },
      },
    };
  };
  const run = async (reason: "rate_limited" | "overloaded") => {
    const inner = createFakeAiClient({
      structured: () => {
        throw new AiUnavailableError(reason);
      },
    });
    const { breaker, failures } = exec();
    const e = await withBreaker(inner, breaker)
      .structured(req())
      .catch((x) => x);
    return { e, failures };
  };
  it("429 does not count as a failure but is still rethrown", async () => {
    const { e, failures } = await run("rate_limited");
    expect(e).toBeInstanceOf(AiUnavailableError);
    expect(e.reason).toBe("rate_limited");
    expect(failures).toEqual([]);
  });
  it("overloaded counts as a failure", async () => {
    const { e, failures } = await run("overloaded");
    expect(e.reason).toBe("overloaded");
    expect(failures).toEqual(["overloaded"]);
  });
});

describe("UnknownModelError", () => {
  it("is thrown for an unpriced model", () => {
    const u = { inputTokens: 1, outputTokens: 1, cacheCreationTokens: 0, cacheReadTokens: 0 };
    expect(() => costMicroUsd("claude-nope" as never, u)).toThrow(UnknownModelError);
    expect(() => costMicroUsd("toString" as never, u)).toThrow(UnknownModelError);
  });
});

describe("custom ids", () => {
  it("round-trips", () => {
    const id = encodeCustomId({ schemaId: "report-section", version: 3, ref: "run12_sec-a" });
    expect(id).toBe("report-section__v3__run12_sec-a");
    expect(decodeCustomId(id)).toEqual({
      schemaId: "report-section",
      version: 3,
      ref: "run12_sec-a",
    });
  });
  it("refs may contain double underscores", () => {
    const id = encodeCustomId({ schemaId: "a", version: 1, ref: "x__y" });
    expect(decodeCustomId(id)?.ref).toBe("x__y");
  });
  it("matches the API charset and length", () => {
    const id = encodeCustomId({ schemaId: "s", version: 1, ref: "r".repeat(40) });
    expect(id).toMatch(/^[A-Za-z0-9_-]{1,64}$/);
    expect(() => encodeCustomId({ schemaId: "s", version: 1, ref: "r".repeat(60) })).toThrow();
    expect(() => encodeCustomId({ schemaId: "a@b", version: 1, ref: "r" })).toThrow();
    expect(() => encodeCustomId({ schemaId: "a_b", version: 1, ref: "r" })).toThrow();
    expect(() => encodeCustomId({ schemaId: "a", version: 1, ref: "r:1" })).toThrow();
  });
  it("decode rejects foreign ids", () => {
    expect(decodeCustomId("plain")).toBeNull();
    expect(decodeCustomId("a__vx__r")).toBeNull();
  });
});

function batchFetch(lines: string, opts: { failAfterFirst?: Error } = {}): typeof fetch {
  return (async (u: unknown, init?: RequestInit) => {
    if (String(u).endsWith("/results")) {
      const enc = new TextEncoder();
      const body = new ReadableStream({
        start(c) {
          const [first, ...rest] = lines.split("\n");
          c.enqueue(enc.encode(`${first}\n`));
          if (opts.failAfterFirst) c.error(opts.failAfterFirst);
          else {
            if (rest.length) c.enqueue(enc.encode(rest.join("\n")));
            c.close();
          }
        },
      });
      return new Response(body, {
        status: 200,
        headers: { "content-type": "application/x-jsonl" },
      });
    }
    if (init?.method === "POST") {
      return new Response(
        JSON.stringify({ id: "batch_9", processing_status: "in_progress", type: "message_batch" }),
        { status: 200, headers: json },
      );
    }
    return new Response(
      JSON.stringify({
        id: "batch_9",
        processing_status: "ended",
        results_url: "https://api.anthropic.com/v1/messages/batches/batch_9/results",
      }),
      { status: 200, headers: json },
    );
  }) as typeof fetch;
}

const line = (custom_id: string, result: unknown) => JSON.stringify({ custom_id, result });
const okLine = (id: string, m: ReturnType<typeof msg>) =>
  line(id, { type: "succeeded", message: m });

async function collect(it: AsyncIterable<BatchResultItem>) {
  const out: BatchResultItem[] = [];
  for await (const r of it) out.push(r);
  return out;
}

describe("cross-process batch schemas", () => {
  it("submit with one instance, read results with a fresh instance", async () => {
    const f = batchFetch(
      [
        okLine("report-section__v1__a", msg('{"section":"s","n":1}')),
        okLine("other__v1__b", msg('{"section":"s","n":1}')),
      ].join("\n"),
    );
    const writer = mk(f);
    const id = await writer.batch.submit([
      { customId: "report-section__v1__a", req: req() },
      { customId: "other__v1__b", req: req() },
    ]);
    const reader = mk(f); // fresh instance, no registry
    const out = await collect(
      reader.batch.results(id, (cid) =>
        decodeCustomId(cid)?.schemaId === "report-section" ? schema : undefined,
      ),
    );
    expect(out[0]?.result?.parsed).toEqual({ section: "s", n: 1 });
    expect(out[0]?.parseError).toBeNull();
    expect(out[1]?.result?.parsed).toBeNull();
    expect(out[1]?.parseError).toBe("no_schema");
  });
});

describe("batch result handling", () => {
  it("refusal, truncation and mismatch are reported like structured", async () => {
    const f = batchFetch(
      [
        okLine("a", msg("no", "refusal")),
        okLine("b", msg('{"sec', "max_tokens")),
        okLine("c", msg('{"section":1}')),
        okLine("d", msg(null)),
        okLine("e", msg("zzz")),
      ].join("\n"),
    );
    const out = await collect(mk(f).batch.results("batch_9", () => schema));
    expect(out.map((o) => o.parseError)).toEqual([
      "refusal",
      "truncated",
      "schema_mismatch",
      "no_text",
      "invalid_json",
    ]);
    expect(out[2]?.result?.parseIssues?.[0]).toEqual({ path: "section", code: "invalid_type" });
  });

  it("errored items carry kind by error type", async () => {
    const err = (t: string) => ({
      type: "errored",
      error: { type: "error", error: { type: t, message: "m" } },
    });
    const f = batchFetch(
      [
        line("a", err("overloaded_error")),
        line("b", err("api_error")),
        line("c", err("invalid_request_error")),
        line("d", err("authentication_error")),
        line("e", err("permission_error")),
        line("f", err("weird_error")),
        line("g", { type: "canceled" }),
      ].join("\n"),
    );
    const out = await collect(mk(f).batch.results("batch_9", () => schema));
    expect(out.map((o) => o.errorKind)).toEqual([
      "retryable",
      "retryable",
      "fatal",
      "fatal",
      "fatal",
      "unknown",
      null,
    ]);
  });

  it("classifies and redacts a stream failure mid-iteration", async () => {
    const f = batchFetch(okLine("a", msg('{"section":"s","n":1}')), {
      failAfterFirst: new TypeError(`socket hang up ${KEY}`),
    });
    const seen: BatchResultItem[] = [];
    let err: unknown;
    try {
      for await (const r of mk(f).batch.results("batch_9", () => schema)) seen.push(r);
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(AiUnavailableError);
    expect((err as AiUnavailableError).reason).toBe("connection");
    expect(String(err)).not.toContain("SECRET123");
  });

  it("classifies a JSONL decode error without leaking content", async () => {
    const f = batchFetch(`${okLine("a", msg('{"section":"s","n":1}'))}\n{not json ${KEY}`);
    let err: unknown;
    try {
      await collect(mk(f).batch.results("batch_9", () => schema));
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(AiResponseError);
    expect(String(err)).not.toContain("SECRET123");
  });
});

describe("fake client resolver path", () => {
  it("scripts schema mismatch, invalid json, refusal and no_schema", async () => {
    const outs = [
      { parsed: { section: 1 } },
      { text: "zzz" },
      { text: "x", stopReason: "refusal" },
      { parsed: { section: "s", n: 1 } },
    ];
    const ai = createFakeAiClient({ structured: (_r, n) => outs[n] ?? {} });
    const results = [];
    for (let i = 0; i < outs.length; i++) results.push(await ai.structured(req()));
    expect(results.map((r) => r.parseError)).toEqual([
      "schema_mismatch",
      "invalid_json",
      "refusal",
      null,
    ]);
    expect(results[0]?.parseIssues?.[0]?.path).toBe("section");

    const b = createFakeAiClient({
      batches: {
        b1: {
          status: "ended",
          results: [
            { customId: "x__v1__a", outcome: "succeeded", text: '{"section":"s","n":1}' },
            { customId: "y__v1__a", outcome: "succeeded", text: '{"section":"s","n":1}' },
            { customId: "z", outcome: "errored", errorType: "overloaded_error" },
          ],
        },
      },
    });
    const out = await collect(
      b.batch.results("b1", (cid) => (cid.startsWith("x") ? schema : undefined)),
    );
    expect(out.map((o) => o.parseError)).toEqual([null, "no_schema", null]);
    expect(out[2]?.errorKind).toBe("retryable");
  });

  it("scripts 429 and overloaded by throwing", async () => {
    const ai = createFakeAiClient({
      structured: () => {
        throw new AiUnavailableError("rate_limited", undefined, { retryAfterMs: 1500 });
      },
    });
    const e = await ai.structured(req()).catch((x) => x);
    expect(e.retryAfterMs).toBe(1500);
    expect(AiAuthError).toBeDefined();
  });
});
