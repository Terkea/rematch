"use client";

import { CaretDownIcon, Icon, InfoIcon, XIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import { PLAYER, WHY } from "./fixtures";
import { GoalProgress } from "./overview";
import { FloatPanel, MockHeader } from "./shell";

// "Why this?" on a recommendation: the facts behind it, the rules that ranked it and the
// algorithm version that made it, so the same games always explain the same way.

const F = PLAYER.focus;
const LINING = "[font-variant-numeric:lining-nums]";

function FocusCard({ why }: { why?: React.ReactNode }) {
  return (
    <section
      data-surface="sheet"
      className="relative flex max-w-3xl flex-col gap-3 rounded-sheet bg-sheet px-(--space-panel-pad) py-5"
    >
      <h2 className="m-0 text-base font-medium text-ink-soft">This week's focus</h2>
      <p className={`m-0 font-serif text-xl font-semibold text-ink ${LINING}`}>
        {F.name}: from {F.rate}% to {F.target}% of chances over your next {F.nextGames} rapid games.
      </p>
      <GoalProgress />
      {why}
    </section>
  );
}

function Facts({ compact = false }: { compact?: boolean }) {
  const rows = compact ? WHY.facts.slice(0, 3) : WHY.facts;
  return (
    <dl className="m-0 flex flex-col">
      {rows.map((f) => (
        <div
          key={f.label}
          className="grid grid-cols-1 gap-0.5 border-b border-rule py-2.5 md:grid-cols-3 md:gap-4"
        >
          <dt className="text-sm text-ink-soft">{f.label}</dt>
          <dd className={`m-0 flex flex-col text-sm md:col-span-2 text-ink ${LINING}`}>
            <span>{f.value}</span>
            <span className="text-xs text-ink-faint">{f.range}</span>
          </dd>
        </div>
      ))}
    </dl>
  );
}

function Version() {
  return (
    <p className={`m-0 text-xs text-ink-faint ${LINING}`}>
      Made by {WHY.algorithm} ({WHY.algorithmId}) on {WHY.computedAt}, from {WHY.inputs}.
    </p>
  );
}

function Rules() {
  return (
    <ul className="m-0 flex list-disc flex-col gap-1 pl-5 text-sm text-ink-soft">
      {WHY.rules.map((r) => (
        <li key={r}>{r}</li>
      ))}
    </ul>
  );
}

/** A: the card opens in place, the facts as a ruled list under the goal. */
export function WhyInline() {
  return (
    <>
      <MockHeader current="/app" sound="header" />
      <main className="mx-auto flex max-w-6xl flex-col gap-6 px-(--space-gutter) py-8">
        <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Overview</h1>
        <FocusCard
          why={
            <div className="flex flex-col gap-4 border-t border-rule pt-4">
              <span className="inline-flex items-center gap-1 text-sm font-medium text-ink">
                <Icon icon={CaretDownIcon} />
                Why this?
              </span>
              <Facts />
              <div className="flex flex-col gap-2">
                <p className="m-0 text-sm font-medium text-ink">How it was ranked</p>
                <Rules />
              </div>
              <Version />
            </div>
          }
        />
      </main>
    </>
  );
}

/** B: a side panel (a bottom sheet on phones) beside the page, the page still in view. */
export function WhyPanel() {
  return (
    <>
      <MockHeader current="/app" sound="header" />
      <main className="relative mx-auto flex max-w-6xl flex-col gap-6 px-(--space-gutter) py-8 lg:flex-row lg:items-start">
        <div className="flex min-w-0 flex-1 flex-col gap-6">
          <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Overview</h1>
          <FocusCard
            why={
              <span className="inline-flex w-fit items-center gap-1 rounded-field bg-well px-2 py-1 text-sm font-medium text-ink">
                <Icon icon={InfoIcon} />
                Why this?
              </span>
            }
          />
        </div>
        <aside
          aria-label="Why this?"
          data-surface="sheet"
          className="flex flex-col gap-5 rounded-sheet border border-rule bg-sheet px-(--space-panel-pad) py-6 shadow-lift lg:w-md lg:shrink-0 dark:border-rule-strong"
        >
          <div className="flex items-start justify-between gap-3">
            <div className="flex flex-col gap-1">
              <p className="m-0 text-sm text-ink-soft">Why this?</p>
              <h2 className="m-0 font-serif text-xl font-semibold text-ink">
                {WHY.recommendation}
              </h2>
            </div>
            <Button variant="quiet" size="sm" aria-label="Close" className="w-8 px-0">
              <Icon icon={XIcon} />
            </Button>
          </div>
          <Facts />
          <div className="flex flex-col gap-2">
            <p className="m-0 text-sm font-medium text-ink">How it was ranked</p>
            <Rules />
          </div>
          <Version />
          <span className="text-sm text-ink underline decoration-rule-strong underline-offset-4">
            See every weakness in this order
          </span>
        </aside>
      </main>
    </>
  );
}

/** C: a short popover on the link (three facts and the version), with a link to the full reasoning. */
export function WhyPopover() {
  return (
    <>
      <MockHeader current="/app" sound="header" />
      <main className="mx-auto flex max-w-6xl flex-col gap-6 px-(--space-gutter) py-8">
        <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Overview</h1>
        <FocusCard
          why={
            <div className="relative">
              <span className="inline-flex items-center gap-1 rounded-field bg-well px-2 py-1 text-sm font-medium text-ink">
                <Icon icon={InfoIcon} />
                Why this?
              </span>
              <div className="relative z-10 mt-1.5 md:absolute md:top-full md:left-0">
                <FloatPanel wide>
                  <p className="m-0 text-base font-medium text-ink">Why this focus</p>
                  <Facts compact />
                  <Version />
                  <span className="text-sm text-ink underline decoration-rule-strong underline-offset-4">
                    The full reasoning
                  </span>
                </FloatPanel>
              </div>
            </div>
          }
        />
        <div className="h-64 max-md:hidden" />
      </main>
    </>
  );
}
