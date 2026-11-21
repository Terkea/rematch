import { classifyTimeControl, type EngineScore, type TimeClass } from "@ci/chess-core";
import type { PlatformClient } from "../http/platform-client.js";
import type { NormalizedGame } from "./types.js";

interface LichessPlayer {
  user?: { name?: string };
  aiLevel?: number;
  rating?: number;
}

interface LichessGame {
  id: string;
  rated?: boolean;
  variant?: string;
  speed?: string;
  createdAt: number;
  status?: string;
  winner?: "white" | "black";
  players: { white: LichessPlayer; black: LichessPlayer };
  opening?: { eco?: string; name?: string };
  moves?: string;
  pgn?: string;
  analysis?: { eval?: number; mate?: number }[];
  clock?: { initial: number; increment: number };
}

const DRAW_STATUSES = new Set(["draw", "stalemate", "outoftime"]);

function toResult(g: LichessGame): NormalizedGame["result"] {
  if (g.winner === "white") return "1-0";
  if (g.winner === "black") return "0-1";
  // outoftime without a winner means timeout vs insufficient material: a draw.
  if (g.status && DRAW_STATUSES.has(g.status)) return "1/2-1/2";
  return "*";
}

function playerOf(p: LichessPlayer): { name: string; rating: number | null } {
  const name =
    p.user?.name ?? (p.aiLevel !== undefined ? `Stockfish level ${p.aiLevel}` : "Anonymous");
  return { name, rating: p.rating ?? null };
}

function toEvals(g: LichessGame): (EngineScore | null)[] | null {
  if (!g.analysis) return null;
  const plyCount = g.moves ? g.moves.split(" ").filter(Boolean).length : g.analysis.length;
  const out: (EngineScore | null)[] = [];
  for (let i = 0; i < plyCount; i++) {
    const a = g.analysis[i];
    if (a?.mate !== undefined) out.push({ mate: a.mate });
    else if (a?.eval !== undefined) out.push({ cp: a.eval });
    else out.push(null);
  }
  return out;
}

function timing(g: LichessGame): {
  timeClass: TimeClass;
  baseSeconds: number | null;
  incrementSeconds: number | null;
} {
  if (g.speed === "correspondence" || !g.clock) {
    return { timeClass: "daily", baseSeconds: null, incrementSeconds: null };
  }
  return {
    timeClass: classifyTimeControl(g.clock.initial, g.clock.increment),
    baseSeconds: g.clock.initial,
    incrementSeconds: g.clock.increment,
  };
}

export async function* fetchLichessGames(
  client: PlatformClient,
  username: string,
  opts: { since?: Date; until?: Date; max?: number; token?: string } = {},
): AsyncIterable<NormalizedGame> {
  const params = new URLSearchParams();
  if (opts.since) params.set("since", String(opts.since.getTime()));
  if (opts.until) params.set("until", String(opts.until.getTime()));
  if (opts.max !== undefined) params.set("max", String(opts.max));
  params.set("sort", "dateDesc");
  params.set("pgnInJson", "true");
  params.set("clocks", "true");
  params.set("evals", "true");
  params.set("opening", "true");
  const url = `https://lichess.org/api/games/user/${encodeURIComponent(username.toLowerCase())}?${params}`;
  const headers: Record<string, string> = { Accept: "application/x-ndjson" };
  if (opts.token) headers.Authorization = `Bearer ${opts.token}`;

  for await (const g of client.getNdjson<LichessGame>(url, { headers })) {
    if (g.variant && g.variant !== "standard") continue;
    if (g.status === "aborted" || g.status === "noStart") continue;
    if (!g.pgn) continue;
    yield {
      platform: "lichess",
      platformGameId: g.id,
      pgn: g.pgn ?? "",
      playedAt: new Date(g.createdAt),
      ...timing(g),
      rated: g.rated ?? false,
      white: playerOf(g.players.white),
      black: playerOf(g.players.black),
      result: toResult(g),
      termination: g.status ?? null,
      eco: g.opening?.eco ?? null,
      openingName: g.opening?.name ?? null,
      lichessEvals: toEvals(g),
    };
  }
}
