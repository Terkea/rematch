"use client";

import type { Annotation } from "@ci/ui/annotations";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { annotationsFor } from "../annotations";
import type { ReviewState } from "../review-state";
import type { ReviewView } from "../view-model";
import { BEAT, type RevealMode, type RevealSchedule, revealSchedule } from "./schedule";
import { seenStore } from "./seen-store";

// The analysis reveal (DESIGN.md Choreography §1, M7, M8, M10; R10 and ADR 0052). One
// requestAnimationFrame clock drives the whole sequence: each frame reads `performance.now()`
// against the run's start and derives the board's state from the schedule, so there are no
// setTimeout chains, and skipping is one assignment (the start goes to −∞, so every frame is past
// the end). The board, the eval bar and the annotation layer animate each state change themselves
// (lift, travel and land; the pen strokes; the bar's swing).

export type RevealPhase = "running" | "done";

export type RevealState = Partial<Pick<ReviewState, "ply">> & {
  /** The marks to draw instead of the review's own (running only). */
  annotations?: Annotation[];
  animateAnnotations: boolean;
  /** Mount the eval graph with its draw-in (the full version's prologue). */
  drawGraph: boolean;
};

export type Reveal = {
  phase: RevealPhase;
  /** Known once a run has started on the client; null before that and when none runs. */
  mode: RevealMode | null;
  /** Changes at every run's start: key the graph on it so its draw-in replays. */
  run: number;
  state: RevealState;
  skip: () => void;
  replay: () => void;
};

/** What the board shows at one instant of a run. `marks` is the ply whose marks are drawn. */
type Frame = { ply: number; marks: number | null };

/** The end state: the worst moment with its marks, or the final position when nothing is marked. */
export function endPly(view: ReviewView): number {
  return view.moments[0] ?? view.plies.length;
}

/** The player's three worst moments in ascending severity (third worst first, worst last). */
export function climaxOf(view: ReviewView): { ply: number }[] {
  return view.moments
    .slice(0, 3)
    .reverse()
    .map((ply) => ({ ply }));
}

/** Every judged ply carries a graph marker ring, the opponent's included (the prologue's `n`). */
function markedCount(view: ReviewView): number {
  return view.plies.filter((p) => p.analysis?.judgement).length;
}

/**
 * The board at `t` ms into a run, before its end. The prologue (and a condensed run, from 0)
 * shows the end position; the prologue holds it without marks while the graph draws. Each beat
 * shows the position before its move from t0, plays the move at t0 + 400, draws its marks from
 * t0 + 860 and, unless it is the last, retracts them at t0 + 1900.
 */
export function frameAt(view: ReviewView, s: RevealSchedule, mode: RevealMode, t: number): Frame {
  const end = endPly(view);
  if (mode === "condensed") return { ply: end, marks: view.moments.length > 0 ? end : null };
  const beat = s.beats.findLast((b) => b.t0 <= t);
  if (!beat) return { ply: end, marks: null };
  const d = t - beat.t0;
  const ply = d < BEAT.play ? beat.ply - 1 : beat.ply;
  const drawn = d >= BEAT.marks && (!beat.retract || d < BEAT.retract);
  return { ply, marks: drawn ? beat.ply : null };
}

type Run = { id: number; mode: RevealMode };

function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}

export function useReveal(
  view: ReviewView,
  gameId: string | undefined,
  opts: { reducedMotion: boolean; explicitPly: boolean },
): Reveal {
  const analysed = view.status === "analysed";
  // Decided from the first render's props only: a ?ply= written later by stepping is not a reason
  // to stop, and reduced motion turning on mid-run skips (below).
  const eligible = useRef(analysed && !opts.explicitPly && gameId !== undefined).current;
  const [phase, setPhase] = useState<RevealPhase>(eligible ? "running" : "done");
  const [run, setRun] = useState<Run | null>(null);
  const [skipped, setSkipped] = useState(false);
  const [frame, setFrame] = useState<Frame>(() => ({ ply: endPly(view), marks: null }));

  // The clock reads the view through a ref: a new view object (a router refresh) must not restart
  // a run mid-way, or start an invisible one after it ended (review m2).
  const viewRef = useRef(view);
  viewRef.current = view;

  /** The run's start on the `performance.now()` timeline; −∞ once skipped. */
  const start = useRef(0);
  const tick = useRef<() => void>(() => {});

  // Start once on the client: here, not in render, so the server and the first paint agree.
  const reduced = opts.reducedMotion;
  const startRun = useCallback((mode: RevealMode) => {
    setSkipped(false);
    setPhase("running");
    setRun((r) => ({ id: (r?.id ?? 0) + 1, mode }));
  }, []);
  // Decided once per component: StrictMode's second effect pass would otherwise find the id it
  // just stored and run the condensed version on a first view.
  const decided = useRef(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: mount only; later changes skip
  useEffect(() => {
    if (decided.current || !eligible || gameId === undefined) return;
    decided.current = true;
    // `useReducedMotion` still holds the server's guess (full motion) during hydration, so ask the
    // media query itself: a reduced-motion user must never get a run that starts and is skipped.
    if (reduced || prefersReducedMotion()) {
      setPhase("done");
      return;
    }
    const seen = seenStore.has(gameId);
    // An id that can't be stored would read "not seen" on every visit: treat it as seen.
    const stored = seenStore.add(gameId);
    startRun(seen || !stored ? "condensed" : "full");
  }, []);

  // The clock.
  useEffect(() => {
    if (!run) return;
    const v = viewRef.current;
    const schedule = revealSchedule({
      markedCount: markedCount(v),
      climax: climaxOf(v),
      mode: run.mode,
    });
    start.current = performance.now();
    let raf = 0;
    let live = true;
    const step = () => {
      if (!live) return;
      const t = performance.now() - start.current;
      if (t >= schedule.total) {
        live = false;
        cancelAnimationFrame(raf);
        setSkipped(!Number.isFinite(start.current));
        setPhase("done");
        return;
      }
      const f = frameAt(v, schedule, run.mode, t);
      setFrame((prev) => (prev.ply === f.ply && prev.marks === f.marks ? prev : f));
      raf = requestAnimationFrame(step);
    };
    tick.current = step;
    step();
    return () => {
      live = false;
      cancelAnimationFrame(raf);
    };
  }, [run]);

  const skip = useCallback(() => {
    start.current = Number.NEGATIVE_INFINITY;
    tick.current();
  }, []);

  const replay = useCallback(() => {
    if (!analysed || reduced) return;
    if (gameId !== undefined) seenStore.add(gameId);
    startRun("full");
  }, [analysed, reduced, gameId, startRun]);

  const running = phase === "running" && run !== null;

  // M7: any key or pointer press skips, and is not consumed: the same event goes on to do its
  // own job (a step, a click on the ledger, a scrub on the graph) from the end state.
  useEffect(() => {
    if (!running) return;
    const onInput = () => skip();
    window.addEventListener("keydown", onInput, true);
    window.addEventListener("pointerdown", onInput, true);
    return () => {
      window.removeEventListener("keydown", onInput, true);
      window.removeEventListener("pointerdown", onInput, true);
    };
  }, [running, skip]);

  // M10: reduced motion turning on mid-run shows the end state.
  useEffect(() => {
    if (reduced && running) skip();
  }, [reduced, running, skip]);

  const marks = useMemo(
    () => (frame.marks === null ? [] : annotationsFor(view, frame.marks, false)),
    [view, frame.marks],
  );

  const full = run?.mode === "full";
  const state: RevealState =
    phase === "running"
      ? { ply: frame.ply, annotations: marks, animateAnnotations: true, drawGraph: full }
      : { animateAnnotations: true, drawGraph: full && !skipped };

  return { phase, mode: run?.mode ?? null, run: run?.id ?? 0, state, skip, replay };
}
