#!/usr/bin/env node
// Writes lab/fixtures/profile-request.json, the hand-built facts of the bridge's contract tests
// (ADR 0106): run it only to change the fixture, then regenerate the golden files with
// `REPLAY_UPDATE=1` (see lab/README.md). The numbers are chosen, not measured:
// - `lab-hangs` hangs a piece at 2% of 50 eligible moves a game, against peers at 0.4%, so W1
//   clears the ranking floor (ADR 0101) and is the focus; its W5 is compared with the evaluation.
// - `lab-clean` matches the peers on W1 and has a W4.missed series with no peer row, so nothing
//   ranks and the focus is "general".
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const NOW = Date.parse("2026-11-01T12:00:00Z");
const DAY = 86_400_000;
const iso = (t) => new Date(t).toISOString().replace(".000Z", "Z");

/** `n` daily points ending a day before NOW, with `events(i)` of `opps` each. */
function series(n, opps, events) {
  return Array.from({ length: n }, (_, i) => ({
    played_at: iso(NOW - (n - i) * DAY),
    events: events(i),
    opportunities: opps,
  }));
}

function stat(metric, median, { pooled = median, epl = 0.3, players = 40, games = 1200 } = {}) {
  return {
    metric,
    median,
    p25: median * 0.6,
    p40: median * 0.85,
    p75: median * 1.5,
    pooledRate: pooled,
    eplPerEvent: epl,
    oppsPerGame: 50,
    oppSlopePer100: null,
    nPlayers: players,
    nGames: games,
    lowSample: false,
  };
}

const reference = {
  "weakness:W1": stat("weakness:W1", 0.004, { epl: 0.35 }),
  "weakness:W9b": stat("weakness:W9b", 0.02, { epl: 0.4 }),
};

const w5 = {
  metric: "weakness:W5",
  median: 0.15,
  p25: 0.15,
  p40: 0.15,
  p75: 0.15,
  pooledRate: 0.15,
  eplPerEvent: 1,
  oppsPerGame: null,
  oppSlopePer100: null,
  nPlayers: 0,
  nGames: 0,
  lowSample: false,
};

const request = {
  schema_version: "lab-v1",
  players: [
    {
      id: "lab-hangs",
      now: iso(NOW),
      band: "B2",
      games_in_window: 60,
      series: {
        W1: series(60, 50, (i) => (i % 5 === 0 ? 2 : i % 2)),
        W5: series(20, 1, (i) => (i % 4 === 0 ? 1 : 0)),
        W9b: series(30, 4, (i) => (i % 10 === 0 ? 1 : 0)),
      },
      reference,
      model_reference: { W5: w5 },
      outcomes: {
        W1: { state: "active", p_imp: 0.12 },
        W9b: { state: "not_an_issue" },
      },
    },
    {
      id: "lab-clean",
      now: iso(NOW),
      band: "B2",
      games_in_window: 45,
      series: {
        W1: series(45, 50, (i) => (i % 5 === 0 ? 1 : 0)),
        "W4.missed": series(15, 2, (i) => (i % 3 === 0 ? 1 : 0)),
      },
      reference,
      model_reference: {},
      outcomes: {},
    },
  ],
};

const out = join(dirname(fileURLToPath(import.meta.url)), "profile-request.json");
writeFileSync(out, `${JSON.stringify(request, null, 2)}\n`);
console.log(`wrote ${out}`);
