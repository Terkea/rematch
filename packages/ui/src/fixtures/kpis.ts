import type { KpiStatProps, Synthetic } from "./types";

/** Synthetic: shaped like spec section 12a, numbers made up until M2 lands. */
export const kpiFixtures: Synthetic & { items: KpiStatProps[] } = {
  synthetic: true,
  items: [
    {
      label: "Accuracy",
      value: 0.742,
      format: "percent",
      interval: { low: 0.718, high: 0.766, level: 0.8 },
      basis: { games: 38, timeClass: "rapid" },
      trend: [0.69, 0.71, 0.7, 0.72, 0.73, null, 0.74, 0.742],
      target: { value: 0.78, label: "Next target" },
    },
    {
      label: "Blunders per game",
      value: 1.84,
      format: "decimal",
      interval: { low: 1.52, high: 2.16, level: 0.8 },
      basis: { games: 38, timeClass: "rapid" },
      trend: [2.6, 2.4, 2.3, 2.1, 2.0, 1.9, 1.84],
      target: { value: 1.5, label: "Next target" },
    },
    {
      label: "Endgame conversion",
      value: 0.612,
      format: "percent",
      interval: { low: 0.48, high: 0.73, level: 0.9 },
      basis: { games: 11, timeClass: "rapid" },
      provisional: true,
      trend: [0.5, 0.55, 0.6, 0.612],
    },
    {
      label: "Score in time scrambles",
      value: 0.53,
      format: "percent",
      interval: { low: 0.3, high: 0.76, level: 0.8 },
      basis: { games: 6, timeClass: "rapid" },
      provisional: true,
      trend: [0.45, null, 0.53],
    },
  ],
};
