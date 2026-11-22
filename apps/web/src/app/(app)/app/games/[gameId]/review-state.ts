import type { ReviewView } from "./view-model";

// The review's state machine (plan, Task 10). Pure: the view drives it and owns the timers.

export type ReviewState = {
  ply: number;
  playing: boolean;
  orientation: "white" | "black";
  showEngine: boolean;
};

export type ReviewAction =
  | { type: "go"; ply: number }
  | { type: "step"; by: 1 | -1 }
  | { type: "first" | "last" | "togglePlay" | "flip" | "toggleEngine" | "tick" }
  | { type: "moment"; dir: 1 | -1 };

/** Autoplay steps once a second. */
export const PLAY_INTERVAL_MS = 1000;

const clamp = (v: ReviewView, ply: number) => Math.min(v.plies.length, Math.max(0, ply));

/** The ply from `?ply=` when it is an integer (clamped), else the worst moment, else the end. */
export function initialState(v: ReviewView, plyParam: string | undefined): ReviewState {
  const fromParam = plyParam !== undefined && /^-?\d+$/.test(plyParam.trim());
  const ply = fromParam ? clamp(v, Number(plyParam)) : (v.moments[0] ?? v.plies.length);
  return { ply, playing: false, orientation: v.playerColor, showEngine: false };
}

/** The moment after (dir 1) or before (dir −1) `ply`, in game order. */
export function momentFrom(v: ReviewView, ply: number, dir: 1 | -1): number | undefined {
  const sorted = [...v.moments].sort((a, b) => a - b);
  return dir === 1 ? sorted.find((m) => m > ply) : sorted.findLast((m) => m < ply);
}

export function reviewReducer(v: ReviewView, s: ReviewState, a: ReviewAction): ReviewState {
  switch (a.type) {
    case "go":
      return { ...s, playing: false, ply: clamp(v, a.ply) };
    case "step":
      return { ...s, playing: false, ply: clamp(v, s.ply + a.by) };
    case "first":
      return { ...s, playing: false, ply: 0 };
    case "last":
      return { ...s, playing: false, ply: v.plies.length };
    case "moment":
      return { ...s, playing: false, ply: momentFrom(v, s.ply, a.dir) ?? s.ply };
    case "togglePlay":
      if (s.playing) return { ...s, playing: false };
      // Play from the start when pressed at the end.
      return { ...s, playing: true, ply: s.ply >= v.plies.length ? 0 : s.ply };
    case "tick": {
      if (!s.playing) return s;
      const ply = clamp(v, s.ply + 1);
      return { ...s, ply, playing: ply < v.plies.length };
    }
    case "flip":
      return { ...s, orientation: s.orientation === "white" ? "black" : "white" };
    case "toggleEngine":
      return { ...s, showEngine: !s.showEngine };
  }
}
