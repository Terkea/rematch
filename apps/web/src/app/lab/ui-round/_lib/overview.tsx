"use client";

import { RatingTrajectory } from "@ci/ui/charts";
import { CaretDownIcon, CaretRightIcon, Icon, InfoIcon, TargetIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import { Sparkline } from "@ci/ui/stats";
import type { ReactNode } from "react";
import { PLAYER, TRAJECTORY_POINTS } from "./fixtures";
import { MockHeader } from "./shell";

// Three overviews that lead with direction (where you are, one focus, practise now) and keep
// measurement behind progressive disclosure. The fictional player of fixtures.ts; the chart and
// buttons are the shipped components.

const P = PLAYER;
const F = P.focus;
const count = new Intl.NumberFormat("en-GB");
const MINUS = "−";
const LINING = "[font-variant-numeric:lining-nums]";
const LINK =
  "rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink";

/** The last rating of each month, for the sparkline. */
const MONTHLY: number[] = (() => {
  const by = new Map<string, number>();
  for (const p of TRAJECTORY_POINTS) by.set(p.date.slice(0, 7), p.rating);
  return [...by.values()];
})();

const sheet = "rounded-sheet bg-sheet px-(--space-panel-pad)";

export function Chart({ className }: { className?: string }) {
  return (
    <RatingTrajectory
      label="Your rapid rating over time"
      points={TRAJECTORY_POINTS}
      peak={{
        date: "2026-03-11T18:00:00Z",
        rating: P.peak.rating,
        label: `Peak ${count.format(P.peak.rating)}, March 2026`,
      }}
      current={{
        date: "2026-11-18T18:00:00Z",
        rating: P.ratingNow,
        label: `Now ${count.format(P.ratingNow)}`,
      }}
      windows={[
        {
          from: "2026-01-07T00:00:00Z",
          to: "2026-03-11T23:59:59Z",
          label: "Then: 30 games",
          short: "Then",
        },
        {
          from: "2026-09-09T00:00:00Z",
          to: "2026-11-18T23:59:59Z",
          label: "Now: 30 games",
          short: "Now",
        },
      ]}
      {...(className ? { className } : {})}
    />
  );
}

/** The goal's progress as a ruled bar of the next 20 games, played ones filled. */
export function GoalProgress() {
  return (
    <div className="flex flex-col gap-2">
      <p className={`m-0 flex items-center gap-1`} aria-hidden="true">
        {Array.from({ length: F.nextGames }, (_, i) => (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed row
            key={i}
            className={`h-2 flex-1 rounded-pill ${i < F.playedSince ? "bg-ink" : "bg-rule"}`}
          />
        ))}
      </p>
      <p className={`m-0 text-sm text-ink-soft ${LINING}`}>
        {F.playedSince} of {F.nextGames} games played this week. {F.missedSince} of {F.chancesSince}{" "}
        mate chances missed so far; too few to judge yet.
      </p>
    </div>
  );
}

export function WhyLink({ open = false }: { open?: boolean }) {
  return (
    <span className="inline-flex items-center gap-1 text-sm text-ink underline decoration-rule-strong underline-offset-4">
      <Icon icon={open ? CaretDownIcon : InfoIcon} />
      Why this?
    </span>
  );
}

/** One row of "More detail": a closed disclosure with a one-line summary. */
function DetailRow({ title, summary }: { title: string; summary: string }) {
  return (
    <li className="flex min-h-14 items-center gap-3 border-b border-rule py-3">
      <Icon icon={CaretRightIcon} className="shrink-0 text-ink-soft" />
      <span className="flex min-w-0 flex-1 flex-col gap-0.5 md:flex-row md:items-baseline md:gap-4">
        <span className="text-base font-medium text-ink md:w-56 md:shrink-0">{title}</span>
        <span className={`text-sm text-ink-soft ${LINING}`}>{summary}</span>
      </span>
    </li>
  );
}

const DETAILS = [
  {
    title: "How you're doing",
    summary: `Accuracy 71.4%, 2.6 blunders a game, last 30 rated rapid games`,
  },
  { title: "Accuracy over time", summary: "Monthly, with an 80% range for each month" },
  { title: "Mistakes per game", summary: "Blunders, mistakes and inaccuracies, White and Black" },
  { title: "By time control", summary: "Rapid 214 games, blitz 96, bullet 12" },
  { title: "Recent games", summary: "The last 10, each opens at its worst moment" },
];

function Practise({ big = false }: { big?: boolean }) {
  return (
    <div className="flex flex-col gap-3">
      <Button size="md" className={`w-fit ${big ? "h-12 px-6 text-lg" : ""}`}>
        Practise {P.practise.due} positions
      </Button>
      <p className={`m-0 text-sm text-ink-soft ${LINING}`}>
        About {P.practise.minutes} minutes, from your own games: mostly mates you missed. Day{" "}
        {P.practise.streakDays} of your streak; one rest day a week keeps it.
      </p>
    </div>
  );
}

function Page({ children }: { children: ReactNode }) {
  return (
    <>
      <MockHeader current="/app" sound="header" />
      <main className="mx-auto flex max-w-6xl flex-col gap-10 px-(--space-gutter) py-8">
        {children}
      </main>
    </>
  );
}

function Title() {
  return (
    <header className="flex flex-col gap-1">
      <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Overview</h1>
      <p className="m-0 text-sm text-ink-soft">Rapid, chess.com. {P.name}</p>
    </header>
  );
}

// ---------------------------------------------------------------- A: three steps, then detail

export function OverviewA() {
  return (
    <Page>
      <Title />
      <ol className="m-0 flex max-w-3xl list-none flex-col gap-4 p-0">
        <li data-surface="sheet" className={`${sheet} flex flex-col gap-3 py-5`}>
          <h2 className="m-0 text-base font-medium text-ink-soft">1. Where you are</h2>
          <p className={`m-0 font-serif text-xl font-semibold text-ink ${LINING}`}>
            Your rapid rating peaked at {count.format(P.peak.rating)} in March and is{" "}
            {count.format(P.ratingNow)} now ({MINUS}
            {Math.abs(P.change)}).
          </p>
          <p className={`m-0 text-sm text-ink-soft ${LINING}`}>
            Accuracy went from {P.before.accuracy}% to {P.now.accuracy}% over 30 games each.{" "}
            <span className={LINK}>See what changed</span>
          </p>
        </li>
        <li data-surface="sheet" className={`${sheet} flex flex-col gap-3 py-5`}>
          <h2 className="m-0 text-base font-medium text-ink-soft">2. This week's focus</h2>
          <p className={`m-0 font-serif text-xl font-semibold text-ink ${LINING}`}>
            {F.name}: from {F.rate}% to {F.target}% of chances over your next {F.nextGames} rapid
            games.
          </p>
          <GoalProgress />
          <WhyLink />
        </li>
        <li data-surface="sheet" className={`${sheet} flex flex-col gap-3 py-5`}>
          <h2 className="m-0 text-base font-medium text-ink-soft">3. Practise now</h2>
          <Practise />
        </li>
      </ol>
      <section aria-labelledby="more" className="flex max-w-3xl flex-col gap-3">
        <h2 id="more" className="m-0 font-serif text-xl font-semibold text-ink">
          More detail
        </h2>
        <p className="m-0 text-sm text-ink-soft">
          Every number behind the three steps above, closed until you open it.
        </p>
        <ul className="m-0 list-none border-t border-rule p-0">
          {DETAILS.map((d) => (
            <DetailRow key={d.title} {...d} />
          ))}
        </ul>
      </section>
    </Page>
  );
}

// ---------------------------------------------------------------- B: the coach's note

export function OverviewB() {
  return (
    <Page>
      <Title />
      <section
        data-surface="sheet"
        className={`${sheet} grid grid-cols-1 gap-8 py-7 lg:grid-cols-12 lg:gap-10`}
      >
        <div className="flex flex-col gap-5 lg:col-span-6">
          <p className={`m-0 font-serif text-2xl leading-snug font-semibold text-ink ${LINING}`}>
            You have lost {Math.abs(P.change)} points since March. The biggest leak is missed mates:{" "}
            {F.missed} of your last {F.chances} chances. This week, work on that one thing.
          </p>
          <p className={`m-0 text-base text-ink-soft ${LINING}`}>
            Goal: {F.target}% or fewer missed over your next {F.nextGames} rapid games (players at
            your level miss {F.peers}%).
          </p>
          <Practise big />
          <WhyLink />
        </div>
        <div className="flex min-w-0 flex-col gap-2 lg:col-span-6">
          <Chart />
          <p className={`m-0 text-xs text-ink-faint ${LINING}`}>
            Based on {P.ratedGames} rated rapid games.
          </p>
        </div>
      </section>
      <section className="flex max-w-3xl flex-col gap-3">
        <h2 className="m-0 font-serif text-xl font-semibold text-ink">This week so far</h2>
        <GoalProgress />
      </section>
      <section className="flex flex-col gap-4">
        <div className="flex items-center gap-1 rounded-field bg-well p-1 text-sm max-md:overflow-x-auto">
          {["Numbers", "Openings", "Endgames", "Time", "Recent games"].map((t) => (
            <span
              key={t}
              className="flex h-8 shrink-0 items-center rounded-field px-3 text-ink-soft"
            >
              {t}
            </span>
          ))}
        </div>
        <p className="m-0 text-sm text-ink-soft">
          The rest of the measurement lives behind these tabs; none is open until you choose one.
        </p>
      </section>
    </Page>
  );
}

// ---------------------------------------------------------------- C: today first

export function OverviewC() {
  return (
    <Page>
      <section className="flex flex-col gap-4">
        <p className="m-0 text-sm text-ink-soft">Today, rapid</p>
        <h1 className={`m-0 font-serif text-4xl font-semibold text-ink ${LINING}`}>
          {P.practise.due} positions to practise
        </h1>
        <p className={`m-0 max-w-prose text-prose text-ink-soft ${LINING}`}>
          About {P.practise.minutes} minutes. Most are mates you missed, this week's focus.
        </p>
        <Button size="md" className="h-12 w-fit px-6 text-lg">
          Start
        </Button>
      </section>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <section data-surface="sheet" className={`${sheet} flex flex-col gap-3 py-5`}>
          <h2 className="m-0 flex items-center gap-2 text-base font-medium text-ink-soft">
            <Icon icon={TargetIcon} />
            This week's focus
          </h2>
          <p className={`m-0 font-serif text-xl font-semibold text-ink ${LINING}`}>
            {F.name}, down to {F.target}%
          </p>
          <GoalProgress />
          <WhyLink />
        </section>
        <section data-surface="sheet" className={`${sheet} flex flex-col gap-3 py-5`}>
          <h2 className="m-0 text-base font-medium text-ink-soft">Where you are</h2>
          <p className={`m-0 flex items-baseline gap-3 ${LINING}`}>
            <span className={`font-serif text-3xl font-semibold text-ink ${LINING}`}>
              {count.format(P.ratingNow)}
            </span>
            <span className="text-sm text-ink-soft">
              {MINUS}
              {Math.abs(P.change)} since the March peak of {count.format(P.peak.rating)}
            </span>
          </p>
          <Sparkline
            values={MONTHLY}
            label={`Rapid rating by month, ${count.format(MONTHLY[0] ?? 0)} to ${count.format(P.ratingNow)}`}
            className="h-12 w-full max-w-sm"
          />
          <p className="m-0 text-sm text-ink-soft">
            Based on {P.ratedGames} rated rapid games.{" "}
            <span className={LINK}>See what changed</span>
          </p>
        </section>
      </div>
      <section className="flex max-w-3xl flex-col gap-2 border-t border-rule pt-6">
        <h2 className="m-0 text-base font-medium text-ink">All your numbers</h2>
        <p className="m-0 text-sm text-ink-soft">
          Accuracy, mistakes, time controls, openings and recent games now live on their own page,
          so this one stays about today.
        </p>
        <Button variant="outline" size="sm" className="w-fit">
          Open your numbers
        </Button>
      </section>
    </Page>
  );
}
