import { describe, expect, it } from "vitest";
import { z } from "zod";
import { withBreaker } from "../../src/client/breaker.js";
import { AiRequestError, AiUnavailableError, classifyAiError } from "../../src/client/errors.js";
import { createFakeAiClient } from "../../src/client/fake.js";
import type { ChatTurnRequest, StructuredRequest } from "../../src/client/types.js";

const sreq: StructuredRequest = {
  model: "claude-haiku-5-5",
  system: [],
  messages: [],
  maxTokens: 10,
  effort: "low",
  schema: z.any(),
};
const treq: ChatTurnRequest = {
  model: "claude-sonnet-5-5",
  system: [],
  tools: [],
  messages: [],
  maxTokens: 10,
  effort: "low",
};

describe("fake client", () => {
  it("records calls and replays deltas in order", async () => {
    const ai = createFakeAiClient({
      structured: (_r, n) => ({ parsed: { n } }),
      turns: [
        () => ({
          deltas: ["a", "b", "c"],
          toolUses: [{ id: "t1", name: "get_eval", input: { x: 1 } }],
          stopReason: "tool_use",
        }),
      ],
    });
    const r = await ai.structured(sreq);
    expect(r.parsed).toEqual({ n: 0 });
    const got: string[] = [];
    const { message } = await ai.streamTurn(treq, (t) => got.push(t));
    expect(got).toEqual(["a", "b", "c"]);
    expect(message.stop_reason).toBe("tool_use");
    expect(message.content.map((b) => b.type)).toEqual(["text", "tool_use"]);
    expect(ai.calls.map((c) => c.kind)).toEqual(["structured", "turn"]);
  });
  it("propagates thrown errors", async () => {
    const ai = createFakeAiClient({
      structured: () => {
        throw new AiUnavailableError("server");
      },
    });
    await expect(ai.structured(sreq)).rejects.toBeInstanceOf(AiUnavailableError);
  });
  it("serves scripted batches", async () => {
    const ai = createFakeAiClient({
      batches: {
        b1: {
          status: "ended",
          results: [{ customId: "x", outcome: "expired", result: null, errorType: null }],
        },
      },
    });
    expect(await ai.batch.status("b1")).toBe("ended");
    const out = [];
    for await (const r of ai.batch.results("b1")) out.push(r);
    expect(out).toHaveLength(1);
    expect(ai.calls.some((c) => c.kind === "batch")).toBe(true);
  });
});

describe("withBreaker", () => {
  it("open breaker gives circuit_open without calling inner", async () => {
    const inner = createFakeAiClient({ structured: () => ({ parsed: 1 }) });
    const ai = withBreaker(inner, {
      exec: async () => {
        throw new Error("open");
      },
    });
    const e = await ai.structured(sreq).catch((x) => x);
    expect(e).toBeInstanceOf(AiUnavailableError);
    expect(e.reason).toBe("circuit_open");
    expect(inner.calls).toHaveLength(0);
  });
  it("passes through when closed and keeps request errors", async () => {
    const inner = createFakeAiClient({
      structured: () => {
        throw new AiRequestError("bad", 400);
      },
    });
    const ai = withBreaker(inner, { exec: (fn) => fn() });
    await expect(ai.structured(sreq)).rejects.toBeInstanceOf(AiRequestError);
  });
  it("only AiUnavailableError counts as a failure (breaker sees the rejection, request errors are wrapped for the breaker as success)", async () => {
    const seen: string[] = [];
    const inner = createFakeAiClient({
      structured: () => {
        throw new AiRequestError("bad", 400);
      },
    });
    const ai = withBreaker(inner, {
      exec: async (fn) => {
        try {
          const v = await fn();
          seen.push("ok");
          return v;
        } catch {
          seen.push("fail");
          throw new Error("x");
        }
      },
    });
    await expect(ai.structured(sreq)).rejects.toBeInstanceOf(AiRequestError);
    expect(seen).toEqual(["ok"]);
  });
});

describe("classifyAiError", () => {
  it("passes through already-classified errors", () => {
    const e = new AiUnavailableError("overloaded");
    expect(classifyAiError(e)).toBe(e);
  });
  it("wraps unknown errors", () => {
    expect(classifyAiError("boom")).toBeInstanceOf(Error);
  });
});
