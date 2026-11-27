import type { GameStatRow } from "../../../server/stats/game-stats";
import { computeTrajectory } from "../../../server/trajectory/model";
import { TrajectorySection } from "../../(app)/app/_trajectory/section";
import { trajectoryView } from "../../(app)/app/_trajectory/view";

// The rating trajectory (F6, ADR 0089) on synthetic games (B15 allows fixtures here): a rise to a
// peak, then a fall below where it started, with accuracy and a weakness worse in the last games. A server component:
// the read model runs here as it does on Progress.

const DAY = 86_400_000;
const START = Date.UTC(2025, 9, 1, 18);

function demo() {
  const n = 160;
  const games = Array.from({ length: n }, (_, i) => ({
    id: `sg-${i}`,
    playedAt: new Date(START + i * 2 * DAY),
    rating: Math.round((i < 90 ? 820 + i * 3.1 : 1099 - (i - 90) * 8) + (((i * 37) % 11) - 5) * 4),
  }));
  const stats: GameStatRow[] = games.map((g, i) => ({
    id: g.id,
    playedAt: g.playedAt,
    color: i % 2 === 0 ? "white" : "black",
    result: (["win", "loss", "win", "draw", "loss"] as const)[i % 5] ?? "win",
    analysed: true,
    accuracy: (i < 130 ? 74 : 64) + ((i * 7) % 9) - 4,
    blunders: (i < 130 ? 1 : 2) + (i % 3 === 0 ? 1 : 0),
    mistakes: 1,
    inaccuracies: 2,
    phaseAccuracy: { opening: null, middlegame: null, endgame: null },
  }));
  const points = new Map(
    games.map((g, i) => [g.id, { events: i < 130 ? i % 3 : 1 + (i % 3), opportunities: 4 }]),
  );
  const t = computeTrajectory({
    games,
    current: { rating: 540, at: new Date(START + n * 2 * DAY) },
    stats,
    weaknesses: [
      {
        seriesKey: "W3.shallow",
        label: "Missing a short tactic",
        prior: { mu: 0.3, n0: 60 },
        points,
      },
    ],
    seed: "styleguide:trajectory",
  });
  return trajectoryView(t, "rapid");
}

export function TrajectoryStyleguideSection() {
  return (
    <section
      id="trajectory"
      data-section="trajectory"
      aria-labelledby="trajectory-title"
      className="flex flex-col gap-6"
    >
      <div className="flex max-w-prose flex-col gap-1">
        <h2 id="trajectory-title" className="font-serif text-2xl font-semibold">
          Trajectory
        </h2>
        <p className="text-sm text-ink-soft">
          Progress opens with the rating line: the peak marked, the compared spans shaded, then the
          first games against the last games. Synthetic games.
        </p>
      </div>
      <TrajectorySection view={demo()} />
    </section>
  );
}
