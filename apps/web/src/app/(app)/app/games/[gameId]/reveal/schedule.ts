// The analysis reveal's timetable (DESIGN.md Choreography §1, binding; ADR 0052). Pure numbers in
// ms from the start of the sequence; `use-reveal.ts` turns them into board states.

export type RevealMode = "full" | "condensed";

export type RevealBeat = { ply: number; t0: number; retract: boolean };

export type RevealSchedule = { total: number; ringStagger: number; beats: RevealBeat[] };

/** The prologue: the graph draws (0–600), then its rings (600–1200). The climax starts here. */
export const PROLOGUE_MS = 1200;
/** The eval graph drawing across (`--dur-reveal`), and the condensed version's whole length. */
export const GRAPH_DRAW_MS = 600;
/** A retracting beat, start to start. */
export const BEAT_MS = 2060;
/** The last beat, which holds its marks and does not retract. */
export const LAST_BEAT_MS = 1900;
/** The ring window both versions fit their graph rings' starts into. */
const RING_WINDOW_MS = 360;

/** Within a beat, from its t0 (Choreography §1's climax table). */
export const BEAT = {
  /** 0–400: cross-fade to the position before the move, then hold. */
  play: 400,
  /** 860: the pen ring starts; the arrow follows and the badge lands by 1400. */
  marks: 860,
  /** 1900: marks retract (first and second beats only). */
  retract: 1900,
} as const;

function stagger(markedCount: number, cap: number): number {
  return markedCount <= 1 ? cap : Math.min(cap, RING_WINDOW_MS / (markedCount - 1));
}

/** `climax` is at most three moments in ascending severity: third worst first, worst last. */
export function revealSchedule(input: {
  markedCount: number;
  climax: { ply: number }[];
  mode: RevealMode;
}): RevealSchedule {
  if (input.mode === "condensed")
    return { total: GRAPH_DRAW_MS, ringStagger: stagger(input.markedCount, 40), beats: [] };
  const k = input.climax.length;
  const beats = input.climax.map((m, i) => ({
    ply: m.ply,
    t0: PROLOGUE_MS + i * BEAT_MS,
    retract: i < k - 1,
  }));
  const total = k === 0 ? GRAPH_DRAW_MS : PROLOGUE_MS + (k - 1) * BEAT_MS + LAST_BEAT_MS;
  return { total, ringStagger: stagger(input.markedCount, 80), beats };
}
