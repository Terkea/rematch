import type { CoachingInput, ExemplarPosition, MoveRef } from "../input/schema.js";
import { extractDates, extractNumbers } from "./extract.js";
import { fenAfter, pvToSan, stripSan } from "./pv.js";
import type { NumFormat, Token } from "./tokens.js";

export interface PositionEvidence {
  gameId: string;
  ply: number;
  fen: string;
  allowedSan: Set<string>;
  allowedUci: Set<string>;
  played: MoveRef | null;
  best: MoveRef | null;
  /** SAN of the legal prefix of bestPv (from fen). */
  bestPvSan: string[];
  /** SAN of the legal prefix of refutationPv (from the position after played). */
  refutationPvSan: string[];
}

export interface Evidence {
  positions: Map<string, PositionEvidence>;
  gameIds: Set<string>;
  lines: Map<string, Set<string>>;
  /** Ordered raw SAN of each opening line (movesSan), for {{line:}} tokens. */
  lineMoves: Map<string, string[]>;
  /** Dotted evidence paths for {{num:}} tokens. */
  numberPaths: Map<string, number>;
  /** Dotted evidence paths for {{date:}} tokens (YYYY-MM-DD). */
  datePaths: Map<string, string>;
  numbers: number[];
  ratios: number[];
  dates: Set<string>;
  addPosition(p: {
    gameId: string;
    ply: number;
    fen: string;
    played: MoveRef | null;
    best: MoveRef | null;
    pvs: { fromFen: string; uci: string[] }[];
    pvSides?: {
      best?: { fromFen: string; uci: string[] };
      refutation?: { fromFen: string; uci: string[] };
    };
  }): void;
  addRatio(r: number): void;
  addNumbersFrom(value: unknown): void;
  addGame(gameId: string): void;
}

const MINED_KEYS = new Set(["label", "description", "name", "before", "after"]);
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const ISO_PREFIX = /^(\d{4})-(\d{2})-(\d{2})(?:$|T)/;

const pad = (n: number) => String(n).padStart(2, "0");
const clean = (n: number) => Number(n.toPrecision(12));

function addSan(set: Set<string>, san: string): void {
  set.add(san);
  set.add(stripSan(san));
}

export function createEvidence(): Evidence {
  const numSet = new Set<number>();
  const ratioSet = new Set<number>();
  const ev: Evidence = {
    positions: new Map(),
    gameIds: new Set(),
    lines: new Map(),
    lineMoves: new Map(),
    numberPaths: new Map(),
    datePaths: new Map(),
    numbers: [],
    ratios: [],
    dates: new Set(),
    addGame(gameId) {
      ev.gameIds.add(gameId);
    },
    addPosition(p) {
      ev.gameIds.add(p.gameId);
      const key = `${p.gameId}#${p.ply}`;
      let pe = ev.positions.get(key);
      if (!pe) {
        pe = {
          gameId: p.gameId,
          ply: p.ply,
          fen: p.fen,
          allowedSan: new Set(),
          allowedUci: new Set(),
          played: p.played,
          best: p.best,
          bestPvSan: [],
          refutationPvSan: [],
        };
        ev.positions.set(key, pe);
      }
      for (const m of [p.played, p.best]) {
        if (!m) continue;
        addSan(pe.allowedSan, m.san);
        pe.allowedUci.add(m.uci);
      }
      if (p.pvSides?.best) pe.bestPvSan = pvToSan(p.pvSides.best.fromFen, p.pvSides.best.uci);
      if (p.pvSides?.refutation) {
        pe.refutationPvSan = pvToSan(p.pvSides.refutation.fromFen, p.pvSides.refutation.uci);
      }
      for (const pv of p.pvs) {
        const sans = pvToSan(pv.fromFen, pv.uci);
        for (const s of sans) addSan(pe.allowedSan, s);
        for (const u of pv.uci.slice(0, sans.length)) pe.allowedUci.add(u);
      }
    },
    addRatio(r) {
      if (Number.isFinite(r) && !ratioSet.has(r)) {
        ratioSet.add(r);
        ev.ratios.push(r);
      }
    },
    addNumbersFrom(value) {
      const addNum = (n: number) => {
        if (!Number.isFinite(n) || numSet.has(n)) return;
        numSet.add(n);
        ev.numbers.push(n);
      };
      const addDate = (y: number | null, mo: number, d: number) => {
        ev.dates.add(`${pad(mo)}-${pad(d)}`);
        if (y !== null) {
          ev.dates.add(`${y}-${pad(mo)}-${pad(d)}`);
          addNum(y);
        }
        addNum(mo);
        addNum(d);
      };
      const walk = (v: unknown, key: string | null): void => {
        if (typeof v === "number") {
          addNum(v);
          if (v >= 0 && v <= 1) addNum(clean(v * 100));
          if (key === "cp") {
            addNum(clean(v / 100));
            addNum(clean(-v / 100));
          } else if (key === "mate") {
            addNum(Math.abs(v));
            addNum(-Math.abs(v));
          } else if (key === "ply" || key === "deviationPly") {
            addNum(Math.ceil(v / 2));
          } else if (key === "baseSeconds") {
            addNum(clean(v / 60));
          }
        } else if (typeof v === "string") {
          if (UUID_RE.test(v)) return;
          const iso = ISO_PREFIX.exec(v);
          if (iso) addDate(Number(iso[1]), Number(iso[2]), Number(iso[3]));
          if (key !== null && MINED_KEYS.has(key)) {
            for (const n of extractNumbers(v)) addNum(n.value);
            for (const d of extractDates(v)) addDate(d.year, d.month, d.day);
          }
        } else if (Array.isArray(v)) {
          addNum(v.length);
          for (const x of v) walk(x, key);
        } else if (typeof v === "object" && v !== null) {
          for (const [k, x] of Object.entries(v)) walk(x, k);
        }
      };
      walk(value, null);
    },
  };
  return ev;
}

function addExemplar(ev: Evidence, e: ExemplarPosition): void {
  const after = fenAfter(e.fen, e.played.uci);
  ev.addPosition({
    gameId: e.gameId,
    ply: e.ply,
    fen: e.fen,
    played: e.played,
    best: e.best,
    pvs: [
      { fromFen: e.fen, uci: e.bestPv },
      ...(after ? [{ fromFen: after, uci: e.refutationPv }] : []),
    ],
    pvSides: {
      best: { fromFen: e.fen, uci: e.bestPv },
      ...(after ? { refutation: { fromFen: after, uci: e.refutationPv } } : {}),
    },
  });
}

export function buildEvidence(input: CoachingInput): Evidence {
  const ev = createEvidence();

  ev.addNumbersFrom(input);

  indexPaths(ev, input);
  for (const w of input.weaknesses) for (const e of w.exemplars) addExemplar(ev, e);
  for (const s of input.strengths) for (const e of s.exemplars) addExemplar(ev, e);
  for (const o of input.openings) {
    for (const e of o.examples) addExemplar(ev, e);
    const line = new Set<string>();
    for (const m of o.movesSan) addSan(line, m);
    if (o.deviationMoveSan) addSan(line, o.deviationMoveSan);
    for (const a of o.alternatives) for (const m of a.movesSan) addSan(line, m);
    ev.lines.set(o.lineKey, line);
    ev.lineMoves.set(o.lineKey, [...o.movesSan]);
  }
  for (const c of input.changes) {
    ev.addGame(c.onsetGameId);
    for (const e of c.examples) addExemplar(ev, e);
    if (c.rateBefore !== 0) ev.addRatio(c.rateAfter / c.rateBefore);
    if (c.rateAfter !== 0) ev.addRatio(c.rateBefore / c.rateAfter);
  }
  return ev;
}

// ---- path index for {{num:}} / {{date:}} ----

const ARRAY_KEYS: Record<string, string> = {
  areaScores: "area",
  metrics: "key",
  weaknesses: "id",
  openings: "lineKey",
  changes: "seriesKey",
};
const seg = (s: string) => s.replace(/[.|:\s]/g, "_");
const SKIP_KEYS = new Set(["bestPv", "refutationPv", "movesSan", "tags", "fen"]);

function indexPaths(ev: Evidence, input: CoachingInput): void {
  const putN = (path: string, n: number) => {
    if (Number.isFinite(n) && !ev.numberPaths.has(path)) ev.numberPaths.set(path, n);
  };
  const walk = (v: unknown, path: string, key: string | null): void => {
    if (typeof v === "number") {
      putN(path, v);
      if (key === "cp") putN(path.replace(/cp$/, "pawns"), clean(v / 100));
      if (key === "ply" || key === "deviationPly")
        putN(
          path.replace(/(ply|deviationPly)$/i, (m) =>
            m === "ply" ? "moveNumber" : "deviationMoveNumber",
          ),
          Math.ceil(v / 2),
        );
      if (key === "baseSeconds") putN(path.replace(/baseSeconds$/, "baseMinutes"), clean(v / 60));
    } else if (typeof v === "string") {
      const iso = ISO_PREFIX.exec(v);
      if (iso && !ev.datePaths.has(path)) ev.datePaths.set(path, v.slice(0, 10));
    } else if (Array.isArray(v)) {
      if (key !== null && SKIP_KEYS.has(key)) return;
      putN(`${path}.count`, v.length);
      const idField = key !== null && path.split(".").length === 1 ? ARRAY_KEYS[key] : undefined;
      v.forEach((x, i) => {
        const id =
          idField && typeof x === "object" && x !== null
            ? (x as Record<string, unknown>)[idField]
            : undefined;
        walk(x, `${path}.${typeof id === "string" ? seg(id) : i}`, key);
      });
    } else if (typeof v === "object" && v !== null) {
      for (const [k, x] of Object.entries(v)) {
        if (SKIP_KEYS.has(k)) continue;
        walk(x, path ? `${path}.${seg(k)}` : seg(k), k);
      }
    }
  };
  walk(input, "", null);
  for (const c of input.changes) {
    const base = `changes.${seg(c.seriesKey)}`;
    if (c.rateBefore !== 0) putN(`${base}.ratioAfterBefore`, c.rateAfter / c.rateBefore);
    if (c.rateAfter !== 0) putN(`${base}.ratioBeforeAfter`, c.rateBefore / c.rateAfter);
  }
}

export function resolveNumberPath(ev: Evidence, path: string): number | undefined {
  return ev.numberPaths.get(path);
}

export function listNumberPaths(ev: Evidence): string[] {
  return [...ev.numberPaths.keys()];
}

export function resolveDatePath(ev: Evidence, path: string): string | undefined {
  return ev.datePaths.get(path);
}

const round = (v: number, d: number) => {
  const f = 10 ** d;
  return Math.round((v + Number.EPSILON * Math.sign(v)) * f) / f;
};
const group = (s: string) => s.replace(/\B(?=(\d{3})+(?!\d))/g, ",");

/** pct0/pct1 treat the value as a fraction (0.31 renders as 31%). */
export function formatNumber(value: number, fmt: NumFormat | null): string {
  switch (fmt) {
    case "int":
      return group(String(Math.round(value)));
    case "pct0":
      return `${round(value * 100, 0).toFixed(0)}%`;
    case "pct1":
      return `${round(value * 100, 1).toFixed(1)}%`;
    case "dec1":
      return round(value, 1).toFixed(1);
    case "dec2":
      return round(value, 2).toFixed(2);
    case "signed1": {
      const r = round(value, 1);
      if (r === 0) return "0.0";
      return `${r > 0 ? "+" : "-"}${Math.abs(r).toFixed(1)}`;
    }
    default:
      return Number.isInteger(value) ? group(String(value)) : String(round(value, 2));
  }
}

/** Renders a token against the evidence, or undefined when it does not resolve. */
export function resolveToken(ev: Evidence, token: Token): string | undefined {
  switch (token.kind) {
    case "move":
      return ev.positions.get(`${token.gameId}#${token.ply}`)?.played?.san;
    case "best":
      return ev.positions.get(`${token.gameId}#${token.ply}`)?.best?.san;
    case "pv": {
      const p = ev.positions.get(`${token.gameId}#${token.ply}`);
      if (!p) return undefined;
      return (token.side === "best" ? p.bestPvSan : p.refutationPvSan)[token.n - 1];
    }
    case "line": {
      const moves = ev.lineMoves.get(token.lineKey);
      if (!moves) return undefined;
      return token.n === null ? moves.join(" ") : moves[token.n - 1];
    }
    case "num": {
      const v = ev.numberPaths.get(token.path);
      return v === undefined ? undefined : formatNumber(v, token.fmt);
    }
    case "date":
      return ev.datePaths.get(token.path);
    case "sq":
      return token.square;
  }
}
