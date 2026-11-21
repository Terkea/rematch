import { classifyTimeControl, type TimeClass } from "@ci/chess-core";
import type { PlatformClient } from "../http/platform-client.js";
import type { NormalizedGame } from "./types.js";

interface ChesscomPlayer {
  username: string;
  rating?: number;
}

interface ChesscomGame {
  url: string;
  pgn: string;
  time_control: string;
  end_time: number;
  rated?: boolean;
  rules?: string;
  white: ChesscomPlayer;
  black: ChesscomPlayer;
}

const ARCHIVE_RE = /\/(\d{4})\/(\d{2})\/?$/;

function header(pgn: string, name: string): string | null {
  const m = new RegExp(`^\\[${name} "((?:[^"\\\\]|\\\\.)*)"\\]`, "m").exec(pgn);
  return m?.[1] ?? null;
}

function toResult(pgn: string): NormalizedGame["result"] {
  const r = header(pgn, "Result");
  return r === "1-0" || r === "0-1" || r === "1/2-1/2" ? r : "*";
}

function timing(tc: string): {
  timeClass: TimeClass;
  baseSeconds: number | null;
  incrementSeconds: number | null;
} {
  const daily = /^1\/(\d+)$/.exec(tc);
  if (daily?.[1]) {
    return { timeClass: "daily", baseSeconds: Number(daily[1]), incrementSeconds: 0 };
  }
  const m = /^(\d+)(?:\+(\d+))?$/.exec(tc);
  if (!m?.[1]) return { timeClass: "daily", baseSeconds: null, incrementSeconds: null };
  const base = Number(m[1]);
  const inc = Number(m[2] ?? 0);
  return {
    timeClass: classifyTimeControl(base, inc),
    baseSeconds: base,
    incrementSeconds: inc,
  };
}

/** Derive an opening name from the chess.com ECOUrl slug, dropping the trailing move list. */
function openingName(pgn: string): string | null {
  const url = header(pgn, "ECOUrl");
  const slug = url?.split("/").pop();
  if (!slug) return null;
  const words = slug.split("-");
  const cut = words.findIndex((w) => /^\d+\./.test(w));
  return (cut === -1 ? words : words.slice(0, cut)).join(" ") || null;
}

function normalize(g: ChesscomGame): NormalizedGame {
  return {
    platform: "chesscom",
    platformGameId: g.url.split("/").filter(Boolean).pop() ?? g.url,
    pgn: g.pgn,
    playedAt: new Date(g.end_time * 1000),
    ...timing(g.time_control),
    rated: g.rated ?? false,
    white: { name: g.white.username, rating: g.white.rating ?? null },
    black: { name: g.black.username, rating: g.black.rating ?? null },
    result: toResult(g.pgn),
    termination: header(g.pgn, "Termination"),
    eco: header(g.pgn, "ECO"),
    openingName: openingName(g.pgn),
    lichessEvals: null,
  };
}

export async function* fetchChesscomGames(
  client: PlatformClient,
  username: string,
  opts: { since?: Date; max?: number } = {},
): AsyncIterable<NormalizedGame> {
  if (opts.max !== undefined && opts.max <= 0) return;
  const base = `https://api.chess.com/pub/player/${encodeURIComponent(username.toLowerCase())}`;
  const { archives } = await client.getJson<{ archives: string[] }>(`${base}/games/archives`);
  const sinceMs = opts.since?.getTime();
  let yielded = 0;

  for (const archive of [...archives].reverse()) {
    const m = ARCHIVE_RE.exec(archive);
    if (sinceMs !== undefined && m?.[1] && m[2]) {
      const monthEnd = Date.UTC(Number(m[1]), Number(m[2]), 1);
      if (monthEnd <= sinceMs) break; // this and all older months are before `since`
    }
    const { games } = await client.getJson<{ games: ChesscomGame[] }>(archive);
    const ordered = [...games].sort((a, b) => b.end_time - a.end_time);
    for (const g of ordered) {
      if (sinceMs !== undefined && g.end_time * 1000 < sinceMs) continue;
      if (g.rules && g.rules !== "chess") continue;
      yield normalize(g);
      yielded++;
      if (opts.max !== undefined && yielded >= opts.max) return;
    }
  }
}
