import type { PositionEval } from "@ci/chess-core";
import { type Db, positionEvals } from "@ci/db";
import { and, eq, inArray } from "drizzle-orm";

export interface EvalCacheEntry {
  fenKey: string;
  protocol: string;
  /** White point of view. */
  eval: PositionEval;
  source: "stockfish" | "lichess";
}

export interface EvalCache {
  getMany(fenKeys: string[], protocol: string): Promise<Map<string, PositionEval>>;
  putMany(entries: EvalCacheEntry[]): Promise<void>;
}

export function createMemoryEvalCache(): EvalCache {
  const rows = new Map<string, PositionEval>();
  const id = (protocol: string, key: string) => `${protocol}\u0000${key}`;
  return {
    async getMany(fenKeys, protocol) {
      const out = new Map<string, PositionEval>();
      for (const k of fenKeys) {
        const hit = rows.get(id(protocol, k));
        if (hit) out.set(k, structuredClone(hit));
      }
      return out;
    },
    async putMany(entries) {
      for (const e of entries) {
        const key = id(e.protocol, e.fenKey);
        if (!rows.has(key)) rows.set(key, structuredClone(e.eval));
      }
    },
  };
}

const CHUNK = 1000;

export function createDbEvalCache(db: Db): EvalCache {
  return {
    async getMany(fenKeys, protocol) {
      const out = new Map<string, PositionEval>();
      const unique = [...new Set(fenKeys)];
      for (let i = 0; i < unique.length; i += CHUNK) {
        const rows = await db
          .select()
          .from(positionEvals)
          .where(
            and(
              eq(positionEvals.protocol, protocol),
              inArray(positionEvals.fenKey, unique.slice(i, i + CHUNK)),
            ),
          );
        for (const r of rows) {
          const score =
            r.scoreMate !== null
              ? { mate: r.scoreMate }
              : r.scoreCp !== null
                ? { cp: r.scoreCp }
                : null;
          if (!score) continue;
          out.set(r.fenKey, { score, bestUci: r.bestUci, pv: r.pv });
        }
      }
      return out;
    },
    async putMany(entries) {
      for (let i = 0; i < entries.length; i += CHUNK) {
        const batch = entries.slice(i, i + CHUNK).map((e) => ({
          fenKey: e.fenKey,
          protocol: e.protocol,
          scoreCp: "cp" in e.eval.score ? e.eval.score.cp : null,
          scoreMate: "mate" in e.eval.score ? e.eval.score.mate : null,
          bestUci: e.eval.bestUci,
          pv: e.eval.pv,
          source: e.source,
        }));
        if (batch.length > 0) await db.insert(positionEvals).values(batch).onConflictDoNothing();
      }
    },
  };
}
