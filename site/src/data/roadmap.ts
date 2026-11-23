// The roadmap, shown on the landing page and on /docs/roadmap/. "Shipped" items cite the decision
// that records them; "Next" is the ranked list from the end-user review of 2026-11-23 (each needs
// the maintainer's approval before work starts); "Open for contributors" are the issues seeded for
// newcomers at launch.

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

export const next: RoadmapItem[] = [
  {
    title: "A daily rhythm",
    detail:
      "A streak that allows one rest day a week, and an opt-in daily email with today's positions.",
  },
  {
    title: "“Is it working?” sooner",
    detail:
      "How many more games until it can tell, and the training numbers you can already trust.",
  },
  {
    title: "The opening-trap line trainer",
    detail: "Replay the exact lines where you fell for a trap, from move 1.",
  },
  {
    title: "Conversion play-outs",
    detail: "Play your won positions on against an engine at your opponent's level (Maia, opt-in).",
  },
  {
    title: "A time-use drill",
    detail: "A clock budget per phase of the game, checked after each game.",
  },
];

export const openForContributors: RoadmapItem[] = [
  {
    title: "Lichess import behind a flag",
    detail: "The importer exists and is tested; accounts were deferred (ADR 0040).",
  },
  { title: "Translations", detail: "i18n scaffolding for the app, then the docs." },
  { title: "Mobile polish", detail: "The app is responsive; the phone deserves a pass." },
  { title: "PGN upload", detail: "A CLI that brings in over-the-board games." },
  {
    title: "A smaller Docker profile",
    detail: "Run the whole stack on a laptop with less memory.",
  },
  { title: "End-to-end tests", detail: "For example, the trajectory's states." },
];
