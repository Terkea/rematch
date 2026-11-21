import {
  type EngineScore,
  fenKey,
  flipScore,
  type ParsedGame,
  type PositionEval,
} from "@ci/chess-core";
import type { EnginePool } from "../engine/engine-pool.js";
import { ANALYSIS_PROTOCOL, ENGINE_NODES } from "../engine/protocol.js";
import type { EvalCache, EvalCacheEntry } from "./eval-cache.js";

export interface EvaluateGameDeps {
  pool: Pick<EnginePool, "analyse">;
  cache: EvalCache;
  /** Parallel engine requests (default 4). The pool queues beyond its own size. */
  concurrency?: number;
}

/** Fixed white-POV score for a final position that is checkmate, stalemate or another draw. */
function terminalEval(game: ParsedGame): PositionEval | null {
  if (game.terminal === null) return null;
  let cp = 0;
  if (game.terminal === "checkmate") {
    // The side to move is mated, so the side that just moved won.
    cp = game.plies.at(-1)?.color === "white" ? 1000 : -1000;
  }
  return { score: { cp }, bestUci: null, pv: [] };
}

/**
 * White-POV evals for every position of the game (`plies.length + 1`). Cache first, then the
 * supplied Lichess evals (`lichessEvals[i]` is the eval after ply i + 1), then the engine.
 */
export async function evaluateGame(
  deps: EvaluateGameDeps,
  game: ParsedGame,
  lichessEvals: (EngineScore | null)[] | null,
): Promise<PositionEval[]> {
  const fens = [game.startFen, ...game.plies.map((p) => p.fenAfter)];
  const keys = fens.map(fenKey);
  const last = fens.length - 1;
  const result: (PositionEval | null)[] = fens.map(() => null);

  const terminal = terminalEval(game);
  if (terminal) result[last] = terminal;

  const wanted = keys.filter((_, i) => result[i] === null);
  const cached = await deps.cache.getMany(wanted, ANALYSIS_PROTOCOL);
  const puts = new Map<string, EvalCacheEntry>();
  const misses = new Map<string, string>(); // fenKey -> fen

  for (let i = 0; i < fens.length; i++) {
    if (result[i] !== null) continue;
    const key = keys[i] as string;
    const hit = cached.get(key);
    if (hit) {
      result[i] = hit;
      continue;
    }
    const lichess = i > 0 ? lichessEvals?.[i - 1] : null;
    if (lichess) {
      const ev: PositionEval = { score: lichess, bestUci: null, pv: [] };
      result[i] = ev;
      if (!puts.has(key)) {
        puts.set(key, { fenKey: key, protocol: ANALYSIS_PROTOCOL, eval: ev, source: "lichess" });
      }
      continue;
    }
    if (!misses.has(key)) misses.set(key, fens[i] as string);
  }

  // Persist Lichess evals first, then each engine result as soon as it resolves, so a failure or
  // crash midway keeps all completed work.
  await deps.cache.putMany([...puts.values()]);

  const analysed = new Map<string, PositionEval>();
  const queue = [...misses];
  let failure: unknown;
  let failed = false;
  const worker = async () => {
    while (!failed) {
      const next = queue.shift();
      if (!next) return;
      const [key, fen] = next;
      try {
        const [line] = await deps.pool.analyse(fen, { nodes: ENGINE_NODES, multipv: 1 });
        if (!line) throw new Error(`Engine returned no analysis for ${fen}`);
        const blackToMove = fen.split(" ")[1] === "b";
        const ev: PositionEval = {
          score: blackToMove ? flipScore(line.score) : line.score,
          bestUci: line.pv[0] ?? null,
          pv: line.pv,
        };
        analysed.set(key, ev);
        await deps.cache.putMany([
          { fenKey: key, protocol: ANALYSIS_PROTOCOL, eval: ev, source: "stockfish" },
        ]);
      } catch (err) {
        if (!failed) failure = err;
        failed = true;
        return;
      }
    }
  };
  const n = Math.max(1, Math.min(deps.concurrency ?? 4, queue.length));
  await Promise.all(Array.from({ length: n }, worker));
  if (failed) throw failure;

  return result.map((r, i) => r ?? (analysed.get(keys[i] as string) as PositionEval));
}
