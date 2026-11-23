// The roadmap, shown on the landing page and on /docs/roadmap/ (ADR 0099). "Shipped" cites the
// decision that records each item. Everything still to do comes from GitHub's milestones and epics,
// read at build time by scripts/sync-content.mjs (src/data/generated/roadmap.json), with the seed
// files in .github/settings as the fallback.
import generated from "./generated/roadmap.json";

export type RoadmapItem = { title: string; detail: string; adr?: string };

export const shipped: RoadmapItem[] = [
  {
    title: "Every move checked by Stockfish 19",
    detail: "Your chess.com games imported, synced every 30 minutes and analysed by a Rust worker.",
    adr: "0030-rust-backend",
  },
  {
    title: "Weaknesses and strengths, explained",
    detail: "Ranked by what they cost, with plain-language meaning and peers at your rating.",
    adr: "0079-explained-stats-and-weakness-story",
  },
  {
    title: "A coach that cites your games",
    detail: "Reports and chat where every fact is a reference, checked before it is shown.",
    adr: "0064-ask-your-coach-chat",
  },
  {
    title: "The missed-moments trainer",
    detail: "Your own mistakes as drills, spaced over 1 to 60 days, with a held-back control.",
    adr: "0090-missed-moments-trainer",
  },
  {
    title: "A weekly focus with a number to hit",
    detail: "One weakness, a target rate and a verdict that waits for enough chances.",
    adr: "0091-weekly-focus-and-goal",
  },
  {
    title: "Rating trajectory, then against now",
    detail: "What changed since your peak, called better or worse only when the ranges separate.",
    adr: "0089-rating-trajectory-and-then-vs-now",
  },
  {
    title: "The coach knows your plan",
    detail: "Chat and the weekly review see your focus and your practice.",
    adr: "0094-coach-knows-the-plan",
  },
];

export type RoadmapEpic = {
  title: string;
  url: string | null;
  state: string;
  /** Sub-issues closed; null when only the seed files were read. */
  done: number | null;
  total: number;
};

export type RoadmapMilestone = {
  title: string;
  description: string;
  horizon: "now" | "next" | "later";
  state: string;
  url: string;
  /** Issues closed; null when only the seed files were read. */
  closed: number | null;
  total: number;
  epics: RoadmapEpic[];
};

export const roadmap = generated as { source: "seed" | "github"; milestones: RoadmapMilestone[] };

export const HORIZONS = [
  { id: "now", title: "Now" },
  { id: "next", title: "Next" },
  { id: "later", title: "Later" },
] as const;

export const milestonesIn = (horizon: RoadmapMilestone["horizon"]) =>
  roadmap.milestones.filter((m) => m.horizon === horizon);

/** "A daily practice rhythm" from "Epic: a daily practice rhythm". */
export const epicName = (title: string) => {
  const name = title.replace(/^Epic: /, "");
  return name.charAt(0).toUpperCase() + name.slice(1);
};

/** "2 of 6 done" when GitHub was read, "6 issues" from the seed files. */
export function progressText(done: number | null, total: number, noun = "issue"): string {
  if (done === null) return `${total} ${noun}${total === 1 ? "" : "s"}`;
  return `${done} of ${total} done`;
}
