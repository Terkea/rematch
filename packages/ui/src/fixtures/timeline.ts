import type { Synthetic, WeaknessTimelineProps } from "./types";

/** Synthetic: shaped like spec section 12b, dates and states made up until M2 lands. */
export const timelineFixtures: Synthetic & WeaknessTimelineProps = {
  synthetic: true,
  range: { from: "2026-03-01", to: "2026-11-01" },
  series: [
    {
      id: "rook-endgames",
      label: "Rook endgames",
      onset: { date: "2026-04-10", low: "2026-03-28", high: "2026-04-24" },
      segments: [
        { state: "not_an_issue", from: "2026-03-01", to: "2026-04-10" },
        { state: "emerging", from: "2026-04-10", to: "2026-05-05" },
        { state: "active", from: "2026-05-05", to: "2026-07-01" },
        { state: "improving", from: "2026-07-01", to: "2026-08-20" },
        { state: "resolved", from: "2026-08-20", to: "2026-10-05" },
        { state: "relapsed", from: "2026-10-05", to: null },
      ],
    },
    {
      id: "time-trouble",
      label: "Time trouble in rapid",
      segments: [
        { state: "active", from: "2026-03-01", to: "2026-06-15" },
        { state: "improving", from: "2026-06-15", to: "2026-09-01" },
        { state: "resolved", from: "2026-09-01", to: null },
      ],
    },
    {
      id: "knight-forks",
      label: "Missed knight forks",
      segments: [
        { state: "not_an_issue", from: "2026-03-01", to: "2026-08-01" },
        { state: "emerging", from: "2026-08-01", to: null },
      ],
    },
  ],
  events: [
    { date: "2026-05-20", kind: "plan", label: "Started rook endgame plan" },
    { date: "2026-06-02", kind: "drill", label: "Lucena drill set" },
  ],
};
