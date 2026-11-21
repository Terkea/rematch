/**
 * Exports one analysed game from the local `chess` database into a UI fixture JSON file.
 *
 * Ownership: W6 owns `scripts/`; this single file is written by M4 Task 3 (real game fixtures).
 *
 * Usage: tsx scripts/export-fixtures.ts --game <platformGameId> --name <fixtureName>
 *
 * The database is only read. The query runs through `psql` inside the compose Postgres container
 * (`chess-improver-postgres-1`) in a read-only session, so the
 * script needs no database driver. Only commit fixtures from public accounts.
 *
 * Evals are converted to White's point of view. `winWhite` is White's win percentage (0 to 100),
 * `cpWhite` and `mateWhite` come from `position_evals` for the position after the move (null when
 * that position was never evaluated, for example after checkmate), and `clockSeconds` is the
 * mover's clock before the move.
 */
import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const requireFromUi = createRequire(resolve(here, "../packages/ui/package.json"));
const { Chess } = requireFromUi("chess.js") as typeof import("chess.js");

const START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const container = "chess-improver-postgres-1";

function arg(name: string): string {
  const i = process.argv.indexOf(`--${name}`);
  const v = i >= 0 ? process.argv[i + 1] : undefined;
  if (!v) throw new Error(`Missing --${name}. Usage: --game <platformGameId> --name <fixtureName>`);
  return v;
}

const gameId = arg("game");
const name = arg("name");
if (!/^[A-Za-z0-9_-]+$/.test(gameId) || !/^[a-z0-9-]+$/.test(name)) {
  throw new Error("--game and --name must be simple identifiers");
}

// Evals are keyed by the position after each move, so they are fetched in a second step.
const evalSql = (keys: string[], protocol: string) => `
SET default_transaction_read_only = on;
SELECT coalesce(jsonb_agg(jsonb_build_object('k', fen_key, 'cp', score_cp, 'mate', score_mate)), '[]'::jsonb)
FROM position_evals
WHERE protocol = '${protocol}' AND fen_key IN (${keys.map((k) => `'${k.replace(/'/g, "''")}'`).join(",")});`;

function psql(query: string): unknown {
  const r = spawnSync(
    "docker",
    [
      "exec",
      "-i",
      container,
      "psql",
      "-U",
      "postgres",
      "-d",
      "chess",
      "-X",
      "-At",
      "-v",
      "ON_ERROR_STOP=1",
    ],
    { input: query, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
  if (r.status !== 0) throw new Error(`psql failed: ${r.stderr}`);
  const line = r.stdout.split("\n").filter((l) => l.startsWith("{") || l.startsWith("["));
  return JSON.parse(line[line.length - 1] ?? "null");
}

interface Row {
  ply: number;
  color: "white" | "black";
  san: string;
  uci: string;
  fen_before: string;
  best_uci: string | null;
  win_after: number;
  judgement: "inaccuracy" | "mistake" | "blunder" | null;
  phase: "opening" | "middlegame" | "endgame";
  clock_before: number | null;
}

const fenKey = (fen: string) => fen.split(" ").slice(0, 4).join(" ");

const base = psql(`
SET default_transaction_read_only = on;
SELECT json_build_object(
  'game', (SELECT row_to_json(g) FROM (
    SELECT id, white_name, black_name, white_rating, black_rating, player_color, result,
           time_class, eco, opening_name, analysis_protocol
    FROM games WHERE platform = 'lichess' AND platform_game_id = '${gameId}') g),
  'moves', (SELECT coalesce(json_agg(m ORDER BY m.ply), '[]'::json) FROM (
    SELECT mv.ply, mv.color, mv.san, mv.uci, mv.fen_before, mv.best_uci, mv.win_after,
           mv.judgement, mv.phase, mv.clock_before
    FROM moves mv JOIN games g ON g.id = mv.game_id
    WHERE g.platform = 'lichess' AND g.platform_game_id = '${gameId}') m))::jsonb;`) as {
  game: {
    white_name: string;
    black_name: string;
    white_rating: number | null;
    black_rating: number | null;
    player_color: "white" | "black";
    result: "1-0" | "0-1" | "1/2-1/2";
    time_class: string;
    eco: string | null;
    opening_name: string | null;
    analysis_protocol: string;
  } | null;
  moves: Row[];
};

if (!base.game || base.moves.length === 0) throw new Error(`No analysed lichess game ${gameId}`);
const g = base.game;

// Positions after each move: the next move's fen_before, or replay the last move with chess.js.
const fensAfter = base.moves.map((m, i) => {
  const next = base.moves[i + 1];
  if (next) return next.fen_before;
  const c = new Chess(m.fen_before);
  c.move(
    m.uci.length === 5
      ? { from: m.uci.slice(0, 2), to: m.uci.slice(2, 4), promotion: m.uci[4] }
      : { from: m.uci.slice(0, 2), to: m.uci.slice(2, 4) },
  );
  return c.fen();
});

const evals = new Map<string, { cp: number | null; mate: number | null }>();
const rows = psql(evalSql(fensAfter.map(fenKey), g.analysis_protocol)) as {
  k: string;
  cp: number | null;
  mate: number | null;
}[];
for (const r of rows) evals.set(r.k, r);

const fixture = {
  id: name,
  source: `https://lichess.org/${gameId}`,
  white: { name: g.white_name, rating: g.white_rating },
  black: { name: g.black_name, rating: g.black_rating },
  playerColor: g.player_color,
  result: g.result,
  timeClass: g.time_class,
  eco: g.eco,
  openingName: g.opening_name,
  startFen: START_FEN,
  plies: base.moves.map((m, i) => {
    const fenAfter = fensAfter[i] as string;
    const e = evals.get(fenKey(fenAfter));
    // position_evals rows are already White's point of view, so no sign flip is needed.
    return {
      ply: m.ply,
      color: m.color,
      san: m.san,
      uci: m.uci,
      fenAfter,
      winWhite: Math.round((m.color === "white" ? m.win_after : 100 - m.win_after) * 100) / 100,
      cpWhite: e?.cp ?? null,
      mateWhite: e?.mate ?? null,
      judgement: m.judgement,
      bestUci: m.best_uci,
      clockSeconds: m.clock_before,
      phase: m.phase,
    };
  }),
};

const out = resolve(here, "../packages/ui/src/fixtures/games", `${name}.json`);
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, `${JSON.stringify(fixture, null, 2)}\n`);
console.log(`wrote ${out} (${fixture.plies.length} plies)`);
