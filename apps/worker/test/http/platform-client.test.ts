import { describe, expect, it } from "vitest";
import {
  createPlatformClient,
  PlatformNotFoundError,
  PlatformRateLimitError,
} from "../../src/http/platform-client.js";

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

const base = { name: "lichess", contactEmail: "ops@example.com", retryAfterMs: 0 } as const;

describe("platform client", () => {
  it("sends identifying user agent", async () => {
    let seen: Headers | undefined;
    const fetchFn = (async (_url: string | URL | Request, init?: RequestInit) => {
      seen = new Headers(init?.headers);
      return json({ ok: true });
    }) as typeof fetch;
    const client = createPlatformClient({ ...base, fetch: fetchFn });
    await client.getJson("https://example.test/a", { headers: { Accept: "application/json" } });
    expect(seen?.get("user-agent")).toBe("chess-improver/0.1 (+mailto:ops@example.com)");
    expect(seen?.get("accept")).toBe("application/json");
  });

  it("serialises requests", async () => {
    let inFlight = 0;
    let max = 0;
    const fetchFn = (async () => {
      inFlight++;
      max = Math.max(max, inFlight);
      await new Promise((r) => setTimeout(r, 10));
      inFlight--;
      return json({});
    }) as typeof fetch;
    const client = createPlatformClient({ ...base, fetch: fetchFn });
    await Promise.all([
      client.getJson("https://example.test/1"),
      client.getJson("https://example.test/2"),
    ]);
    expect(max).toBe(1);
  });

  it("retries after 429", async () => {
    let calls = 0;
    const fetchFn = (async () => {
      calls++;
      return calls === 1 ? new Response("slow down", { status: 429 }) : json({ n: 1 });
    }) as typeof fetch;
    const client = createPlatformClient({ ...base, fetch: fetchFn });
    await expect(client.getJson("https://example.test/a")).resolves.toEqual({ n: 1 });
    expect(calls).toBe(2);
  });

  it("waits retryAfterMs between attempts", async () => {
    let calls = 0;
    const times: number[] = [];
    const fetchFn = (async () => {
      times.push(Date.now());
      return ++calls === 1 ? new Response("", { status: 429 }) : json({});
    }) as typeof fetch;
    const client = createPlatformClient({ ...base, retryAfterMs: 50, fetch: fetchFn });
    await client.getJson("https://example.test/a");
    expect((times[1] ?? 0) - (times[0] ?? 0)).toBeGreaterThanOrEqual(45);
  });

  it("throws PlatformRateLimitError after four 429s", async () => {
    let calls = 0;
    const fetchFn = (async () => {
      calls++;
      return new Response("", { status: 429 });
    }) as typeof fetch;
    const client = createPlatformClient({ ...base, fetch: fetchFn });
    await expect(client.getJson("https://example.test/a")).rejects.toBeInstanceOf(
      PlatformRateLimitError,
    );
    expect(calls).toBe(4);
  });

  it("404 throws PlatformNotFoundError", async () => {
    const fetchFn = (async () => new Response("", { status: 404 })) as typeof fetch;
    const client = createPlatformClient({ ...base, fetch: fetchFn });
    await expect(client.getJson("https://example.test/a")).rejects.toBeInstanceOf(
      PlatformNotFoundError,
    );
  });

  it("getNdjson yields three objects from a body split mid-line", async () => {
    const enc = new TextEncoder();
    const chunks = ['{"a":1}\n{"a"', ':2}\n{"a":3', "}\n"];
    const fetchFn = (async () =>
      new Response(
        new ReadableStream({
          start(c) {
            for (const ch of chunks) c.enqueue(enc.encode(ch));
            c.close();
          },
        }),
      )) as typeof fetch;
    const client = createPlatformClient({ ...base, fetch: fetchFn });
    const out: unknown[] = [];
    for await (const o of client.getNdjson("https://example.test/s")) out.push(o);
    expect(out).toEqual([{ a: 1 }, { a: 2 }, { a: 3 }]);
  });
});
