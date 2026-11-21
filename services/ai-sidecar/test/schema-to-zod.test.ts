import * as sdk from "@anthropic-ai/claude-agent-sdk";
import { describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { runChat } from "../src/chat.js";
import { SchemaUnsupported, type SidecarError } from "../src/errors.js";
import { Limiter } from "../src/limiter.js";
import { parseChatRequest } from "../src/options.js";
import { jsonSchemaToZodShape } from "../src/schema-to-zod.js";
import { captureLogger, FakeRes, testConfig } from "./helpers.js";

vi.mock("@anthropic-ai/claude-agent-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@anthropic-ai/claude-agent-sdk")>()),
  query: vi.fn(),
}));

const T22 = {
  type: "object",
  properties: {
    gameId: { type: "string", description: "game" },
    ply: { type: "integer", minimum: 0, maximum: 600 },
    moves: { type: "array", items: { type: "string" } },
    side: { type: "string", enum: ["white", "black"] },
    score: { type: "number" },
    deep: { type: "boolean" },
  },
  required: ["gameId", "ply"],
  additionalProperties: false,
};

describe("jsonSchemaToZodShape", () => {
  it("converts_the_t22_subset", () => {
    const obj = z.object(jsonSchemaToZodShape(T22)).strict();
    expect(obj.parse({ gameId: "g1", ply: 4 })).toEqual({ gameId: "g1", ply: 4 });
    expect(
      obj.parse({ gameId: "g", ply: 0, moves: ["e4"], side: "black", score: 1.5, deep: true }),
    ).toMatchObject({ moves: ["e4"], side: "black" });
    expect(obj.safeParse({ ply: 4 }).success).toBe(false); // gameId required
    expect(obj.safeParse({ gameId: "g", ply: -1 }).success).toBe(false); // minimum
    expect(obj.safeParse({ gameId: "g", ply: 601 }).success).toBe(false); // maximum
    expect(obj.safeParse({ gameId: "g", ply: 1.5 }).success).toBe(false); // integer
    expect(obj.safeParse({ gameId: "g", ply: 1, side: "red" }).success).toBe(false); // enum
    expect(obj.safeParse({ gameId: "g", ply: 1, moves: [1] }).success).toBe(false);
    // The SDK re-derives JSON Schema from the shape: the required list survives the round trip.
    const json = z.toJSONSchema(z.object(jsonSchemaToZodShape(T22))) as { required?: string[] };
    expect(json.required?.sort()).toEqual(["gameId", "ply"]);
  });

  it("rejects_unsupported_constructs", () => {
    const cases: Array<[unknown, string]> = [
      [{ ...T22, properties: { a: { oneOf: [{ type: "string" }] } } }, "$.properties.a"],
      [{ ...T22, properties: { a: { type: "object", properties: {} } } }, "$.properties.a"],
      [{ ...T22, properties: { a: { $ref: "#/defs/x" } } }, "$.properties.a"],
      [{ ...T22, additionalProperties: true }, "$.additionalProperties"],
      [
        { ...T22, properties: { a: { type: "array", items: { type: "integer" } } } },
        "$.properties.a.items",
      ],
      [{ type: "array" }, "$"],
      [{ ...T22, required: ["nope"] }, "$.required"],
    ];
    for (const [schema, path] of cases) {
      try {
        jsonSchemaToZodShape(schema);
        expect.unreachable(`accepted ${JSON.stringify(schema)}`);
      } catch (e) {
        expect(e).toBeInstanceOf(SchemaUnsupported);
        expect((e as SchemaUnsupported).path).toBe(path);
        expect((e as SidecarError).status).toBe(400);
      }
    }
  });

  it("chat_with_unsupported_schema_is_400_without_query", async () => {
    vi.mocked(sdk.query).mockReset();
    const req = parseChatRequest({
      model: "claude-sonnet-5-5",
      effort: "low",
      maxTokens: 1000,
      timeoutMs: 1000,
      maxTurns: 3,
      systemPrompt: "s",
      prompt: [{ type: "text", text: "hi" }],
      conversationId: "6f1c3c1e-8d3b-4c1e-9a65-1f2b3c4d5e6f",
      tools: [{ name: "bad", description: "d", inputSchema: { type: "object", oneOf: [] } }],
      toolCallback: { url: "http://127.0.0.1:1/v1/tools", token: "t" },
    });
    const res = new FakeRes();
    const e = await runChat(req, testConfig(), res, {
      limiter: new Limiter(1, 1, 1000),
      logger: captureLogger(),
    }).catch((x) => x);
    expect(e).toBeInstanceOf(SchemaUnsupported);
    expect(res.headersSent).toBe(false);
    expect(sdk.query).not.toHaveBeenCalled();
  });
});
