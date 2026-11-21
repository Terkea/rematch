import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildCoachServer, buildCoachTools, CALLBACK_TIMEOUT_MS } from "../src/tools.js";

type Seen = { auth?: string; body: unknown; path?: string };

async function callbackServer(
  handler: (req: IncomingMessage, res: ServerResponse, body: unknown) => void,
): Promise<{ url: string; seen: Seen[]; server: Server }> {
  const seen: Seen[] = [];
  const server = createServer((req, res) => {
    let raw = "";
    req.on("data", (c) => {
      raw += c;
    });
    req.on("end", () => {
      const body = JSON.parse(raw);
      seen.push({ auth: req.headers.authorization, body, path: req.url });
      handler(req, res, body);
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const { port } = server.address() as AddressInfo;
  return { url: `http://127.0.0.1:${port}/v1/tools`, seen, server };
}

const SPEC = [
  {
    name: "get_position",
    description: "Position at a ply",
    inputSchema: {
      type: "object",
      properties: { gameId: { type: "string" }, ply: { type: "integer", minimum: 0 } },
      required: ["gameId", "ply"],
      additionalProperties: false,
    },
  },
];
const UNAVAILABLE = {
  content: [
    { type: "text", text: JSON.stringify({ available: false, reason: "tool unavailable" }) },
  ],
  isError: true,
};

const servers: Server[] = [];
afterEach(() => {
  vi.useRealTimers();
  for (const s of servers.splice(0)) s.close();
});

describe("coach tools", () => {
  it("tool_call_posts_name_and_input_with_turn_token", async () => {
    const cb = await callbackServer((_q, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ content: "FEN rnbqkbnr/...", isError: false }));
    });
    servers.push(cb.server);
    const [def] = buildCoachTools(SPEC, { url: cb.url, token: "turn-token-1" });
    const out = await def?.handler({ gameId: "g1", ply: 12 }, {});
    expect(out).toEqual({ content: [{ type: "text", text: "FEN rnbqkbnr/..." }], isError: false });
    expect(cb.seen).toEqual([
      {
        auth: "Bearer turn-token-1",
        body: { name: "get_position", input: { gameId: "g1", ply: 12 } },
        path: "/v1/tools",
      },
    ]);
  });

  it("reply_is_error_flag_passes_through", async () => {
    const cb = await callbackServer((_q, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ content: "no such game", isError: true }));
    });
    servers.push(cb.server);
    const [def] = buildCoachTools(SPEC, { url: cb.url, token: "t" });
    expect(await def?.handler({ gameId: "g", ply: 1 }, {})).toEqual({
      content: [{ type: "text", text: "no such game" }],
      isError: true,
    });
  });

  it("callback_error_gives_is_error_tool_unavailable", async () => {
    // 500
    const cb = await callbackServer((_q, res) => {
      res.writeHead(500);
      res.end("internal detail http://secret-host");
    });
    servers.push(cb.server);
    const [d500] = buildCoachTools(SPEC, { url: cb.url, token: "t" });
    expect(await d500?.handler({ gameId: "g", ply: 1 }, {})).toEqual(UNAVAILABLE);

    // a malformed 200
    const bad = await callbackServer((_q, res) => {
      res.writeHead(200);
      res.end("not json");
    });
    servers.push(bad.server);
    const [dBad] = buildCoachTools(SPEC, { url: bad.url, token: "t" });
    expect(await dBad?.handler({ gameId: "g", ply: 1 }, {})).toEqual(UNAVAILABLE);

    // connection refused
    const gone = await callbackServer(() => {});
    await new Promise((r) => gone.server.close(r));
    const [dRefused] = buildCoachTools(SPEC, { url: gone.url, token: "t" });
    expect(await dRefused?.handler({ gameId: "g", ply: 1 }, {})).toEqual(UNAVAILABLE);

    // a 56 s stall: the 55 s callback timeout fires first
    vi.useFakeTimers();
    let sawAbort = false;
    const stall: typeof fetch = (_u, init) =>
      new Promise((_r, reject) => {
        init?.signal?.addEventListener("abort", () => {
          sawAbort = true;
          reject(new Error("aborted"));
        });
        setTimeout(
          () => _r(new Response(JSON.stringify({ content: "late", isError: false }))),
          56000,
        );
      });
    const [dSlow] = buildCoachTools(
      SPEC,
      { url: "http://127.0.0.1:9/v1/tools", token: "t" },
      stall,
    );
    const p = dSlow?.handler({ gameId: "g", ply: 1 }, {});
    await vi.advanceTimersByTimeAsync(56000);
    expect(await p).toEqual(UNAVAILABLE);
    expect(sawAbort).toBe(true);
    expect(CALLBACK_TIMEOUT_MS).toBe(55000);
  });

  it("server_is_named_coach_with_a_60s_timeout", () => {
    const s = buildCoachServer(SPEC, { url: "http://127.0.0.1:9/v1/tools", token: "t" });
    expect(s).toMatchObject({ type: "sdk", name: "coach", timeout: 60000 });
    expect(s.instance).toBeDefined();
  });
});
