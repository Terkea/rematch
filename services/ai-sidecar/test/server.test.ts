import type { Server } from "node:http";
import type { AddressInfo } from "node:net";
import * as sdk from "@anthropic-ai/claude-agent-sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Limiter } from "../src/limiter.js";
import { Metrics } from "../src/log.js";
import { createSidecarServer } from "../src/server.js";
import { captureLogger, result, scripted, TOKEN, testConfig, textDelta } from "./helpers.js";

vi.mock("@anthropic-ai/claude-agent-sdk", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@anthropic-ai/claude-agent-sdk")>()),
  query: vi.fn(),
}));

const queryBody = {
  model: "claude-sonnet-5-5",
  effort: "medium",
  maxTokens: 16000,
  timeoutMs: 600000,
  maxTurns: 3,
  systemPrompt: "SYS",
  prompt: [{ type: "text", text: "x" }],
  outputFormat: { type: "json_schema", schema: { type: "object" } },
};
const { outputFormat: _o, ...chatBase } = queryBody;
const chatBody = {
  ...chatBase,
  conversationId: "6f1c3c1e-8d3b-4c1e-9a65-1f2b3c4d5e6f",
  tools: [],
  toolCallback: { url: "http://127.0.0.1:9/v1/tools", token: "t" },
};

let server: Server | undefined;
async function start(
  env: NodeJS.ProcessEnv = {},
  limiter = new Limiter(4, 32, 30000),
  shutdown?: AbortSignal,
) {
  const cfg = testConfig(env);
  server = createSidecarServer(cfg, {
    limiter,
    logger: captureLogger(),
    metrics: new Metrics(),
    ...(shutdown ? { shutdown } : {}),
  });
  await new Promise<void>((r) => server?.listen(0, "127.0.0.1", r));
  const { port } = server.address() as AddressInfo;
  return `http://127.0.0.1:${port}`;
}
const post = (url: string, body: unknown, token: string | null = TOKEN) =>
  fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(token === null ? {} : { authorization: `Bearer ${token}` }),
    },
    body: JSON.stringify(body),
  });

beforeEach(() => {
  vi.mocked(sdk.query).mockReset();
});
afterEach(async () => {
  await new Promise((r) => (server ? server.close(r) : r(undefined)));
  server = undefined;
});

describe("server", () => {
  it("missing_or_wrong_bearer_is_401", async () => {
    const s = scripted([[result()]]);
    vi.mocked(sdk.query).mockImplementation(s.fn as never);
    const base = await start();
    for (const tok of [null, "wrong", `${TOKEN}x`, ""]) {
      for (const path of ["/v1/query", "/v1/chat"]) {
        const r = await post(base + path, queryBody, tok);
        expect(r.status).toBe(401);
        expect(await r.json()).toEqual({
          type: "error",
          error: { type: "authentication_error", message: expect.any(String) },
        });
      }
    }
    expect(s.calls).toHaveLength(0);
    const ok = await post(`${base}/v1/query`, queryBody);
    expect(ok.status).toBe(200);
    expect(await ok.json()).toMatchObject({ subtype: "success" });
  });

  it("health_needs_no_token_and_reports_auth_without_secrets", async () => {
    const base = await start();
    const r = await fetch(`${base}/health`);
    expect(r.status).toBe(200);
    const text = await r.text();
    expect(JSON.parse(text)).toEqual({
      ok: true,
      auth: "api_key",
      inFlight: 0,
      queued: 0,
      maxConcurrency: 4,
      sdkVersion: "0.3.325",
      mirrorErrors: 0,
      nulStrippedEntries: 0,
    });
    expect(text).not.toContain("SECRET");
    expect(text).not.toContain(TOKEN);
    expect(sdk.query).not.toHaveBeenCalled();
  });

  it("unknown_route_is_404_and_bad_json_is_400", async () => {
    const base = await start();
    const r = await post(`${base}/v1/nope`, {});
    expect(r.status).toBe(404);
    expect(((await r.json()) as { error: { type: string } }).error.type).toBe("not_found_error");
    const bad = await fetch(`${base}/v1/query`, {
      method: "POST",
      headers: { authorization: `Bearer ${TOKEN}` },
      body: "{not json",
    });
    expect(bad.status).toBe(400);
    expect(((await bad.json()) as { error: { type: string } }).error.type).toBe(
      "invalid_request_error",
    );
    const model = await post(`${base}/v1/query`, { ...queryBody, model: "sonnet" });
    expect(model.status).toBe(400);
  });

  it("api_error_status_429_forwards_retry_after", async () => {
    const s = scripted([
      [
        {
          type: "system",
          subtype: "api_retry",
          retry_delay_ms: 3000,
          error_status: 429,
        } as never,
        result({ is_error: true, api_error_status: 429 }),
      ],
    ]);
    vi.mocked(sdk.query).mockImplementation(s.fn as never);
    const base = await start();
    const r = await post(`${base}/v1/query`, queryBody);
    expect(r.status).toBe(429);
    expect(r.headers.get("retry-after")).toBe("3");
    expect(((await r.json()) as { error: { type: string } }).error.type).toBe("rate_limit_error");
  });

  it("overloaded_is_529_with_retry_after", async () => {
    let release!: () => void;
    const s = scripted([
      async function* () {
        await new Promise<void>((r) => {
          release = r;
        });
        yield result();
      },
    ]);
    vi.mocked(sdk.query).mockImplementation(s.fn as never);
    const base = await start({}, new Limiter(1, 0, 30000));
    const first = post(`${base}/v1/query`, queryBody);
    await vi.waitFor(() => expect(s.calls).toHaveLength(1));
    for (const path of ["/v1/query", "/v1/chat"]) {
      const r = await post(base + path, path === "/v1/chat" ? chatBody : queryBody);
      expect(r.status).toBe(529);
      expect(r.headers.get("retry-after")).toBe("5");
      expect(await r.json()).toEqual({
        type: "error",
        error: { type: "overloaded_error", message: expect.any(String) },
      });
    }
    release();
    expect((await first).status).toBe(200);
  });

  it("chat_streams_sse_over_http", async () => {
    const s = scripted([[textDelta("hi"), result()]]);
    vi.mocked(sdk.query).mockImplementation(s.fn as never);
    const base = await start();
    const r = await post(`${base}/v1/chat`, chatBody);
    expect(r.status).toBe(200);
    expect(r.headers.get("content-type")).toBe("text/event-stream");
    const text = await r.text();
    expect(text).toContain('event: delta\ndata: {"text":"hi"}\n\n');
    expect(text).toContain("event: done\n");
  });

  it("no_credentials_in_development_is_401_authentication_error", async () => {
    const base = await start({ ANTHROPIC_API_KEY: "" });
    const h = (await (await fetch(`${base}/health`)).json()) as { auth: string };
    expect(h.auth).toBe("none");
    for (const [path, body] of [
      ["/v1/query", queryBody],
      ["/v1/chat", chatBody],
    ] as const) {
      const r = await post(base + path, body);
      expect(r.status).toBe(401);
      expect(((await r.json()) as { error: { type: string } }).error.type).toBe(
        "authentication_error",
      );
    }
    expect(sdk.query).not.toHaveBeenCalled();
  });

  it("sigterm_aborts_in_flight_calls_with_503", async () => {
    let aborted = false;
    const s = scripted([
      async function* (signal) {
        await new Promise((r) => signal.addEventListener("abort", r));
        aborted = true;
        throw new Error("aborted");
      },
    ]);
    vi.mocked(sdk.query).mockImplementation(s.fn as never);
    const shutdown = new AbortController();
    const base = await start({}, new Limiter(4, 32, 30000), shutdown.signal);
    const pending = post(`${base}/v1/query`, queryBody);
    await vi.waitFor(() => expect(s.calls).toHaveLength(1));
    shutdown.abort();
    const r = await pending;
    expect(aborted).toBe(true);
    expect(r.status).toBe(503);
  });
});
