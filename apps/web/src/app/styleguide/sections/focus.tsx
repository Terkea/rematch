import { FocusCard } from "../../../components/focus/focus-card";
import type { FocusCard as FocusCardModel } from "../../../server/focus";
import { focusProgress } from "../../../server/focus/goal";

// The weekly focus card (ADR 0091) in its states. Demo numbers for the styleguide only: the app
// shows the player's own, never these.

const ACCOUNT = "00000000-0000-4000-8000-000000000001";
const focus = {
  id: "demo",
  accountId: ACCOUNT,
  timeClass: "rapid" as const,
  seriesKey: "W5",
  side: "both" as const,
  label: "Missed mates",
  targetRate: 0.15,
  gamesN: 20,
  startedAt: new Date("2026-11-16T09:00:00Z"),
  baseline: { rate: 0.2, events: 18, chances: 90 },
  peerRate: 0.1,
  endedAt: null,
  outcome: null,
  algorithmId: null,
};
const suggestion = {
  accountId: ACCOUNT,
  timeClass: "rapid" as const,
  side: "both" as const,
  seriesKey: "W5",
  label: "Missed mates",
  baseline: { rate: 0.2, events: 18, chances: 90 },
  peer: 0.1,
  target: { rate: 0.15, basis: "relative" as const },
  gamesN: 20,
};

const STATES: { title: string; card: FocusCardModel }[] = [
  { title: "Suggested", card: { kind: "suggest", suggestion } },
  {
    title: "Running, too early to tell",
    card: {
      kind: "active",
      focus,
      progress: focusProgress({ k: 1, n: 6, games: 7, gamesN: 20, target: 0.15 }),
    },
  },
  {
    title: "Running, on track",
    card: {
      kind: "active",
      focus,
      progress: focusProgress({ k: 1, n: 13, games: 14, gamesN: 20, target: 0.15 }),
    },
  },
  {
    title: "Closed",
    card: {
      kind: "closed",
      focus: { ...focus, outcome: "met", endedAt: new Date("2026-11-22T18:00:00Z") },
      progress: focusProgress({ k: 2, n: 19, games: 20, gamesN: 20, target: 0.15 }),
      next: { ...suggestion, seriesKey: "W1", label: "Blunders" },
    },
  },
];

export function FocusSection() {
  return (
    <section
      id="focus"
      data-section="focus"
      aria-labelledby="focus-title"
      className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex max-w-prose flex-col gap-1">
        <h2 id="focus-title" className="font-serif text-2xl font-semibold">
          Weekly focus
        </h2>
        <p className="text-sm text-ink-soft">
          One weakness, a target rate per chance over the next games, and an honest verdict that
          waits for enough chances. Demo numbers.
        </p>
      </div>
      <div className="grid gap-6 lg:grid-cols-2">
        {STATES.map((s) => (
          <div key={s.title} className="flex flex-col gap-2">
            <h3 className="m-0 text-sm font-medium text-ink-soft">{s.title}</h3>
            <div className="rounded-sheet bg-paper p-3">
              <FocusCard card={s.card} />
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
