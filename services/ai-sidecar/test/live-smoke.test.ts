// Live smoke test against the real Claude Agent SDK and the real `claude` binary.
// It is skipped unless AI_LIVE_SMOKE=1. Never enable it in CI: it spends real tokens and needs a
// credential (ANTHROPIC_API_KEY, or CLAUDE_CODE_OAUTH_TOKEN for the owner's local development),
// plus DATABASE_URL_TEST for a throwaway session-store database. Example (compose worker):
//   AI_LIVE_SMOKE=1 ANTHROPIC_API_KEY=... pnpm --filter @ci/ai-sidecar exec vitest run test/live-smoke.test.ts
import { randomBytes, randomUUID } from "node:crypto";
import type { Server } from "node:http";
import type { AddressInfo } from "node:net";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { loadConfig } from "../src/config.js";
import { Limiter } from "../src/limiter.js";
import { Metrics } from "../src/log.js";
import { createSidecarServer } from "../src/server.js";
import { PgSessionStore } from "../src/session-store.js";
import { createStoreDb, type StoreDb } from "./db.js";
import { captureLogger, tempDir } from "./helpers.js";

const LIVE = process.env.AI_LIVE_SMOKE === "1";
const MODEL = "claude-haiku-5-5";
type Usage = Record<string, { inputTokens: number; outputTokens: number }>;
type Done = {
  subtype: string;
  session: string;
  text: string;
  modelUsage: Usage;
  sessionModelUsage: Usage;
};

describe.skipIf(!LIVE)("live smoke (AI_LIVE_SMOKE=1 only, never in CI)", () => {
  const token = randomBytes(24).toString("hex");
  let db: StoreDb;
  let server: Server;
  let base: string;

  beforeAll(async () => {
    const adminUrl = process.env.DATABASE_URL_TEST;
    if (!adminUrl) throw new Error("the live smoke test needs DATABASE_URL_TEST");
    db = await createStoreDb(adminUrl);
    const cfg = loadConfig({
      ...process.env,
      AI_SIDECAR_TOKEN: token,
      AI_SIDECAR_ENV: "development",
      AI_SIDECAR_WORK_DIR: tempDir("ai-live-work-"),
      AI_SIDECAR_CONFIG_DIR: tempDir("ai-live-claude-"),
    });
    if (cfg.auth === "none") throw new Error("the live smoke test needs a model credential");
    server = createSidecarServer(cfg, {
      limiter: new Limiter(2, 4, 60000),
      logger: captureLogger(),
      metrics: new Metrics(),
      store: new PgSessionStore(db.pool),
    });
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  }, 60000);

  afterAll(async () => {
    await new Promise((r) => (server ? server.close(r) : r(undefined)));
    await db?.close();
  });

  const post = (path: string, body: unknown) =>
    fetch(base + path, {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
      body: JSON.stringify(body),
    });
  const common = {
    model: MODEL,
    effort: "low",
    maxTokens: 1024,
    timeoutMs: 180000,
    maxTurns: 2,
  };

  async function chat(body: Record<string, unknown>): Promise<Done> {
    const r = await post("/v1/chat", {
      ...common,
      systemPrompt: "You are a terse chess coach. Answer in one short sentence.",
      tools: [],
      toolCallback: { url: "http://127.0.0.1:9/v1/tools", token: "unused" },
      ...body,
    });
    expect(r.status).toBe(200);
    const events = (await r.text())
      .split("\n\n")
      .filter((b) => b.startsWith("event:"))
      .map((b) => {
        const [ev, data] = b.split("\n");
        return { event: ev?.slice(7), data: JSON.parse(data?.slice(6) ?? "null") };
      });
    const terminal = events.filter((e) => e.event === "done" || e.event === "error");
    expect(terminal).toHaveLength(1);
    expect(terminal[0]?.event).toBe("done");
    return terminal[0]?.data as Done;
  }

  it("structured_query_works_with_no_tools_and_dont_ask", async () => {
    const r = await post("/v1/query", {
      ...common,
      systemPrompt: "Return only the requested JSON.",
      prompt: [{ type: "text", text: "Name the most common first move for White in chess." }],
      outputFormat: {
        type: "json_schema",
        schema: {
          type: "object",
          properties: { move: { type: "string" } },
          required: ["move"],
          additionalProperties: false,
        },
      },
    });
    expect(r.status).toBe(200);
    const body = (await r.json()) as {
      subtype: string;
      structuredOutput?: { move?: unknown };
      modelUsage: Usage;
    };
    expect(body.subtype).toBe("success");
    expect(typeof body.structuredOutput?.move).toBe("string");
    expect(Object.values(body.modelUsage).some((u) => u.outputTokens > 0)).toBe(true);
  }, 240000);

  it("chat_new_then_resumed_with_store_rows_and_per_turn_usage", async () => {
    const conversationId = randomUUID();
    const first = await chat({
      conversationId,
      resume: false,
      prompt: [{ type: "text", text: "Remember the word: Najdorf. Reply with OK." }],
    });
    expect(first).toMatchObject({ subtype: "success", session: "new" });

    const { rows } = await db.dbAdmin.query(
      "SELECT count(*)::int AS n FROM ai_sdk_session_entries WHERE session_id = $1",
      [conversationId],
    );
    expect(rows[0].n).toBeGreaterThan(0);

    const second = await chat({
      conversationId,
      resume: true,
      rebuildContext: "<history>The user asked you to remember a word.</history>",
      prompt: [{ type: "text", text: "Which word did I ask you to remember?" }],
    });
    expect(second).toMatchObject({ subtype: "success", session: "resumed" });
    expect(second.text.toLowerCase()).toContain("najdorf");

    // Per-turn usage excludes turn one; the SDK's session total includes it.
    const outputs = (u: Usage) => Object.values(u).reduce((s, x) => s + x.outputTokens, 0);
    expect(outputs(second.modelUsage)).toBeGreaterThan(0);
    expect(outputs(second.sessionModelUsage)).toBeGreaterThan(outputs(second.modelUsage));

    const third = await chat({
      conversationId,
      resume: true,
      rebuildContext:
        "<history>The user asked you to remember a word, then asked for it.</history>",
      prompt: [{ type: "text", text: "Spell that word backwards." }],
    });
    expect(third).toMatchObject({ subtype: "success", session: "resumed" });

    // The metering rule (ADR 0062): on a resumed turn Rust meters S3 - S2 per model. It must agree
    // with the turn's own stream sum, which is the fallback.
    for (const [model, s3] of Object.entries(third.sessionModelUsage)) {
      const s2 = second.sessionModelUsage[model] ?? { inputTokens: 0, outputTokens: 0 };
      const streamed = third.modelUsage[model] ?? { inputTokens: 0, outputTokens: 0 };
      const near = (a: number, b: number) => Math.abs(a - b) <= Math.max(5, 0.1 * b);
      expect(near(s3.outputTokens - s2.outputTokens, streamed.outputTokens)).toBe(true);
      expect(near(s3.inputTokens - s2.inputTokens, streamed.inputTokens)).toBe(true);
    }
  }, 720000);
});
