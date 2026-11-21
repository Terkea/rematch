import { readFileSync } from "node:fs";
import { parsePgn } from "@ci/chess-core";
import { describe, expect, it } from "vitest";
import type { PlatformClient } from "../../src/http/platform-client.js";
import { fetchChesscomGames } from "../../src/importers/chesscom.js";

interface RawGame {
  url: string;
  pgn: string;
  time_control: string;
  end_time: number;
  rules: string;
  [k: string]: unknown;
}

const read = (name: string) =>
  JSON.parse(readFileSync(new URL(`../fixtures/${name}`, import.meta.url), "utf8"));
const archives: { archives: string[] } = read("chesscom-archives.json");
const month: { games: RawGame[] } = read("chesscom-month.json");
const octUrl = archives.archives[1] as string;
const novUrl = archives.archives[2] as string;
const archivesUrl = "https://api.chess.com/pub/player/hikaru/games/archives";

function fakeClient(months: Record<string, RawGame[]> = {}) {
  const urls: string[] = [];
  const data: Record<string, unknown> = {
    [archivesUrl]: archives,
    [novUrl]: month,
    ...Object.fromEntries(Object.entries(months).map(([k, v]) => [k, { games: v }])),
  };
  const client: PlatformClient = {
    async getJson<T>(url: string) {
      urls.push(url);
      if (url in data) return data[url] as T;
      if (archives.archives.includes(url)) return { games: [] } as T;
      throw new Error(`unexpected url ${url}`);
    },
    getNdjson: () => {
      throw new Error("unexpected getNdjson");
    },
  };
  return { client, urls };
}

async function collect<T>(it: AsyncIterable<T>): Promise<T[]> {
  const out: T[] = [];
  for await (const x of it) out.push(x);
  return out;
}

const octGames: RawGame[] = [
  { ...(month.games[0] as RawGame), url: "https://www.chess.com/game/live/1", end_time: 1000 },
  { ...(month.games[0] as RawGame), url: "https://www.chess.com/game/live/2", end_time: 2000 },
];

describe("fetchChesscomGames", () => {
  it("walks archives newest first with a lower-cased username", async () => {
    const { client, urls } = fakeClient({ [octUrl]: octGames });
    const games = await collect(fetchChesscomGames(client, "Hikaru"));
    expect(urls.slice(0, 3)).toEqual([archivesUrl, novUrl, octUrl]);
    expect(games).toHaveLength(7);
    expect(games.slice(-2).map((g) => g.platformGameId)).toEqual(["2", "1"]);
  });

  it("yields games within a month newest first and skips chess960", async () => {
    const { client } = fakeClient();
    const games = await collect(fetchChesscomGames(client, "hikaru"));
    const ends = games.map((g) => g.playedAt.getTime());
    expect(ends).toEqual([...ends].sort((a, b) => b - a));
    expect(games).toHaveLength(5); // 4 real + daily; chess960 skipped
    expect(games.map((g) => g.platformGameId)).not.toContain("900000000001");
  });

  it("stops at max without fetching more months", async () => {
    const { client, urls } = fakeClient({ [octUrl]: octGames });
    const games = await collect(fetchChesscomGames(client, "hikaru", { max: 2 }));
    expect(games).toHaveLength(2);
    expect(urls).toEqual([archivesUrl, novUrl]);
  });

  it("does not fetch months entirely before since", async () => {
    const { client, urls } = fakeClient();
    const games = await collect(
      fetchChesscomGames(client, "hikaru", { since: new Date("2026-11-01T00:00:00Z") }),
    );
    expect(urls).toEqual([archivesUrl, novUrl]);
    expect(games).toHaveLength(5);
  });

  it("filters games older than since within a month", async () => {
    const { client } = fakeClient();
    const cut = month.games[2]?.end_time as number;
    const games = await collect(
      fetchChesscomGames(client, "hikaru", { since: new Date(cut * 1000) }),
    );
    expect(games.length).toBeGreaterThan(0);
    expect(games.length).toBeLessThan(5);
    expect(games.every((g) => g.playedAt.getTime() >= cut * 1000)).toBe(true);
  });

  it("maps increments, daily and ids", async () => {
    const inc = { ...(month.games[0] as RawGame), time_control: "180+2" };
    const { client } = fakeClient({ [novUrl]: [inc, month.games[5] as RawGame] });
    const games = await collect(fetchChesscomGames(client, "hikaru"));
    expect(games.find((g) => g.timeClass !== "daily")).toMatchObject({
      timeClass: "blitz",
      baseSeconds: 180,
      incrementSeconds: 2,
      platform: "chesscom",
      platformGameId: "187517419600",
      lichessEvals: null,
    });
    const daily = games.find((g) => g.timeClass === "daily");
    expect(daily).toMatchObject({ baseSeconds: 86400, incrementSeconds: 0 });
    expect(daily?.platformGameId).toBe("900000000002");
  });

  it("maps 1/259200 to daily with base 259200", async () => {
    const g = { ...(month.games[5] as RawGame), time_control: "1/259200" };
    const { client } = fakeClient({ [novUrl]: [g] });
    const [r] = await collect(fetchChesscomGames(client, "hikaru", { max: 1 }));
    expect(r).toMatchObject({ timeClass: "daily", baseSeconds: 259200, incrementSeconds: 0 });
  });

  it("skips games with an unparseable time_control", async () => {
    const bad = {
      ...(month.games[0] as RawGame),
      time_control: "-",
      url: "https://www.chess.com/game/live/777",
    };
    const { client } = fakeClient({ [novUrl]: [bad, month.games[1] as RawGame] });
    const games = await collect(fetchChesscomGames(client, "hikaru", { max: 5 }));
    expect(games.map((g) => g.platformGameId)).not.toContain("777");
    expect(games.every((g) => g.timeClass !== "daily")).toBe(true);
  });

  it("takes result and metadata from the PGN", async () => {
    const { client } = fakeClient();
    const games = await collect(fetchChesscomGames(client, "hikaru"));
    const g = games.find((x) => x.platformGameId === "187517419600");
    expect(g?.result).toBe("1-0");
    expect(g?.white).toEqual({ name: "Hikaru", rating: 3459 });
    expect(g?.eco).toBe("A01");
    expect(g?.openingName).toBe("Nimzowitsch Larsen Attack Classical Variation");
    expect(g?.termination).toBe("Hikaru won by resignation");
    expect(g?.playedAt.getTime()).toBe(1795051401 * 1000);
  });

  it("recorded standard games parse with a clock on every ply", async () => {
    const { client } = fakeClient();
    const games = await collect(fetchChesscomGames(client, "hikaru"));
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
