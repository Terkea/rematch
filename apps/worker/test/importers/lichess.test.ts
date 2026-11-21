import { readFileSync } from "node:fs";
import { parsePgn } from "@ci/chess-core";
import { describe, expect, it } from "vitest";
import type { PlatformClient, RequestInitLite } from "../../src/http/platform-client.js";
import { fetchLichessGames } from "../../src/importers/lichess.js";

// biome-ignore lint/suspicious/noExplicitAny: raw JSON fixtures
type Raw = Record<string, any>;

const fixture: Raw[] = readFileSync(
  new URL("../fixtures/lichess-games.ndjson", import.meta.url),
  "utf8",
)
  .trim()
  .split("\n")
  .map((l) => JSON.parse(l));

function fakeClient(lines: Raw[] = fixture) {
  const calls: { url: string; init?: RequestInitLite }[] = [];
  const client: PlatformClient = {
    getJson: async () => {
      throw new Error("unexpected getJson");
    },
    async *getNdjson<T>(url: string, init?: RequestInitLite) {
      calls.push({ url, init });
      for (const l of lines) yield l as T;
    },
  };
  return { client, calls };
}

async function collect<T>(it: AsyncIterable<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const x of it) out.push(x);
  return out;
}

describe("fetchLichessGames", () => {
  it("yields standard games and skips variants", async () => {
    const { client } = fakeClient();
    const games = await collect(fetchLichessGames(client, "DrNykterstein", { max: 3 }));
    expect(games).toHaveLength(3);
    expect(games.map((g) => g.platformGameId)).not.toContain("chess960x");
    expect(games[0]?.platform).toBe("lichess");
  });

  it("builds the request URL with lower-cased username and flags", async () => {
    const { client, calls } = fakeClient();
    await collect(fetchLichessGames(client, "DrNykterstein", { max: 3 }));
    const url = calls[0]?.url ?? "";
    expect(url).toContain("/api/games/user/drnykterstein?");
    for (const p of [
      "pgnInJson=true",
      "clocks=true",
      "evals=true",
      "opening=true",
      "sort=dateDesc",
      "max=3",
    ])
      expect(url).toContain(p);
    expect(calls[0]?.init?.headers?.Accept).toBe("application/x-ndjson");
    expect(calls[0]?.init?.headers?.Authorization).toBeUndefined();
  });

  it("passes since as epoch ms and token as bearer", async () => {
    const { client, calls } = fakeClient();
    const since = new Date("2026-01-01T00:00:00Z");
    await collect(fetchLichessGames(client, "x", { since, token: "tok" }));
    expect(calls[0]?.url).toContain(`since=${since.getTime()}`);
    expect(calls[0]?.init?.headers?.Authorization).toBe("Bearer tok");
  });

  it("maps fields of a recorded game", async () => {
    const { client } = fakeClient();
    const [g] = await collect(fetchLichessGames(client, "DrNykterstein", { max: 3 }));
    const raw = fixture[0] as Raw;
    expect(g?.platformGameId).toBe(raw.id);
    expect(g?.timeClass).toBe("blitz");
    expect(g?.baseSeconds).toBe(180);
    expect(g?.incrementSeconds).toBe(0);
    expect(g?.playedAt.getTime()).toBe(raw.createdAt);
    expect(g?.white).toEqual({ name: "DrNykterstein", rating: raw.players.white.rating });
    expect(g?.result).toBe("1-0");
    expect(g?.termination).toBe("resign");
    expect(g?.eco).toBe("A50");
    expect(g?.openingName).toBe("Queen's Indian Accelerated");
    expect(g?.rated).toBe(raw.rated);
  });

  it("derives result from winner/status", async () => {
    const base = fixture[0] as Raw;
    const mk = (id: string, patch: Raw): Raw => {
      const { winner: _w, ...rest } = base;
      return { ...rest, id, ...patch };
    };
    const { client } = fakeClient([
      mk("a", { winner: "black" }),
      mk("b", { status: "draw" }),
      mk("c", { status: "stalemate" }),
      mk("d", { status: "started" }),
    ]);
    const games = await collect(fetchLichessGames(client, "x"));
    expect(games.map((g) => g.result)).toEqual(["0-1", "1/2-1/2", "1/2-1/2", "*"]);
  });

  it("maps correspondence games to daily with null clocks", async () => {
    const { clock: _c, ...rest } = fixture[0] as Raw;
    const { client } = fakeClient([{ ...rest, speed: "correspondence" }]);
    const [g] = await collect(fetchLichessGames(client, "x"));
    expect(g?.timeClass).toBe("daily");
    expect(g?.baseSeconds).toBeNull();
    expect(g?.incrementSeconds).toBeNull();
  });

  it("maps analysis to lichessEvals, one per ply, padding with null", async () => {
    const { client } = fakeClient();
    const games = await collect(fetchLichessGames(client, "x"));
    for (const [i, g] of games.entries()) {
      const plies = parsePgn(g.pgn).plies.length;
      expect(g.lichessEvals).toHaveLength(plies);
      expect(g.lichessEvals?.[0]).toEqual({ cp: (fixture[i] as Raw).analysis[0].eval });
    }
    const raw = fixture[0] as Raw;
    const withMate = {
      ...raw,
      analysis: [{ eval: 10 }, { mate: -3, judgment: { name: "Blunder" } }],
    };
    const { client: c2 } = fakeClient([withMate]);
    const [g] = await collect(fetchLichessGames(c2, "x"));
    expect(g?.lichessEvals?.[0]).toEqual({ cp: 10 });
    expect(g?.lichessEvals?.[1]).toEqual({ mate: -3 });
    expect(g?.lichessEvals?.[2]).toBeNull();
    expect(g?.lichessEvals).toHaveLength(raw.moves.split(" ").length);
  });

  it("has null lichessEvals without analysis", async () => {
    const { analysis: _a, ...rest } = fixture[0] as Raw;
    const { client } = fakeClient([rest]);
    const [g] = await collect(fetchLichessGames(client, "x"));
    expect(g?.lichessEvals).toBeNull();
  });

  it("recorded standard games parse with a clock on every ply", async () => {
    const { client } = fakeClient();
    const games = await collect(fetchLichessGames(client, "x"));
    expect(games.length).toBeGreaterThan(0);
    for (const g of games) {
      const { plies } = parsePgn(g.pgn);
      expect(plies.length).toBeGreaterThan(0);
      if (g.timeClass !== "daily") {
        expect(plies.every((p) => p.clockSeconds !== null)).toBe(true);
      }
    }
  });
});
