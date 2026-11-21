import type { AreaScoreProps, Synthetic } from "./types";

const rapid = (games: number) => ({ games, timeClass: "rapid" });

/** Synthetic: shaped like spec section 8b, numbers made up until M2 lands. */
export const areaScoreFixtures: Synthetic & { items: AreaScoreProps[] } = {
  synthetic: true,
  items: [
    { area: "openings", score: 71, band: { median: 64, next: 72 }, delta: 3, basis: rapid(38) },
    { area: "middlegame", score: 58, band: { median: 60, next: 68 }, delta: -2, basis: rapid(38) },
    { area: "endgame", score: 62, band: { median: 55, next: 68 }, delta: 4, basis: rapid(22) },
    { area: "tactics", score: 66, band: { median: 61, next: 70 }, basis: rapid(38) },
    { area: "calculation", score: 49, band: { median: 57, next: 65 }, delta: 1, basis: rapid(30) },
    { area: "time", score: 74, band: { median: 62, next: 71 }, basis: rapid(38) },
    { area: "mindset", score: 55, band: { median: 58, next: 66 }, delta: -5, basis: rapid(38) },
  ],
};
