import type { CoachingInput, ExemplarPosition, MoveRef } from "../input/schema.js";
import { extractDates, extractNumbers } from "./extract.js";
import { fenAfter, pvToSan, stripSan } from "./pv.js";

export interface PositionEvidence {
  gameId: string;
  ply: number;
  fen: string;
  allowedSan: Set<string>;
  allowedUci: Set<string>;
}

export interface Evidence {
  positions: Map<string, PositionEvidence>;
  gameIds: Set<string>;
  lines: Map<string, Set<string>>;
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
  }): void;
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
        };
        ev.positions.set(key, pe);
      }
      for (const m of [p.played, p.best]) {
        if (!m) continue;
        addSan(pe.allowedSan, m.san);
        pe.allowedUci.add(m.uci);
      }
      for (const pv of p.pvs) {
        const sans = pvToSan(pv.fromFen, pv.uci);
        for (const s of sans) addSan(pe.allowedSan, s);
        for (const u of pv.uci.slice(0, sans.length)) pe.allowedUci.add(u);
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
  // ratios are written by buildEvidence via this helper
  Object.defineProperty(ev, "addRatio", {
    value: (r: number) => {
      if (Number.isFinite(r) && !ratioSet.has(r)) {
        ratioSet.add(r);
        ev.ratios.push(r);
      }
    },
    enumerable: false,
  });
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
  });
}

export function buildEvidence(input: CoachingInput): Evidence {
  const ev = createEvidence();
  const addRatio = (ev as unknown as { addRatio: (r: number) => void }).addRatio;

  ev.addNumbersFrom(input);

  for (const w of input.weaknesses) for (const e of w.exemplars) addExemplar(ev, e);
  for (const s of input.strengths) for (const e of s.exemplars) addExemplar(ev, e);
  for (const o of input.openings) {
    for (const e of o.examples) addExemplar(ev, e);
    const line = new Set<string>();
    for (const m of o.movesSan) addSan(line, m);
    if (o.deviationMoveSan) addSan(line, o.deviationMoveSan);
    for (const a of o.alternatives) for (const m of a.movesSan) addSan(line, m);
    ev.lines.set(o.lineKey, line);
  }
  for (const c of input.changes) {
    ev.addGame(c.onsetGameId);
    for (const e of c.examples) addExemplar(ev, e);
    if (c.rateBefore !== 0) addRatio(c.rateAfter / c.rateBefore);
    if (c.rateAfter !== 0) addRatio(c.rateBefore / c.rateAfter);
  }
  return ev;
}
