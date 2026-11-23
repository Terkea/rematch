import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import type { CoachScope } from "../../../../server/profile";
import { DAILY_CAP, HOLDOUT_PERCENT, SOURCES } from "../../../../server/train/model";
import type { TrainPage } from "../../../../server/train/store";
import { EmptySheet, PageHeading } from "../../_shell/page-heading";
import { SectionHead } from "../_overview/section-head";
import { ScopeRow } from "../coach/scope-row";
import { patternText, rateText, SOURCE_LABEL, sourceRowText } from "./copy";
import { TrainSession } from "./train-session";

// `/app/train` (ADR 0090): today's session of the player's own missed moments, the summary once
// it is done, and "Is it working?". The motif of a position is never named before the attempt.

export const LEDE =
  "Positions from your own games where a better move was there. Find it, then see the engine's line.";
export const NO_SCOPE_TEXT =
  "No analysed games yet. Your practice positions come from your analysed games.";
export const NO_ITEMS_TEXT =
  "No positions to practise yet. They come from your analysed games: a mate you missed, a tactic you missed, or a blunder.";

const count = new Intl.NumberFormat("en-GB");
const positions = (n: number) => `${count.format(n)} ${n === 1 ? "position" : "positions"}`;

function query(scope: CoachScope, extra: Record<string, string> = {}) {
  return new URLSearchParams({
    account: scope.accountId,
    tc: scope.timeClass,
    ...extra,
  }).toString();
}

/** "N due tomorrow", the next session's size (never past the daily cap). */
export function dueTomorrowText(n: number): string {
  const shown = Math.min(n, DAILY_CAP);
  return shown === 0 ? "Nothing due tomorrow yet." : `${positions(shown)} due tomorrow.`;
}

export function TrainView({ scope, page }: { scope: CoachScope | null; page: TrainPage | null }) {
  return (
    <div className="flex flex-col gap-12">
      <div className="flex flex-col gap-5">
        <PageHeading title="Train" lede={LEDE} />
        {scope && scope.options.length > 1 ? (
          <ScopeRow>
            <CoachScopeSwitch scope={scope} basePath="/app/train" />
          </ScopeRow>
        ) : null}
        {scope && page?.source ? (
          <p className="m-0 flex flex-wrap items-center gap-x-3 gap-y-1 text-base text-ink-soft">
            <span>Practising {SOURCE_LABEL[page.source]} only.</span>
            <Link
              href={`/app/train?${query(scope)}`}
              className="rounded-field text-ink underline underline-offset-4 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
            >
              Practise everything
            </Link>
          </p>
        ) : null}
      </div>
      {scope === null || page === null ? (
        <EmptySheet
          action={
            <Button asChild variant="outline">
              <Link href="/app/accounts">Your accounts</Link>
            </Button>
          }
        >
          {NO_SCOPE_TEXT}
        </EmptySheet>
      ) : (
        <Body scope={scope} page={page} />
      )}
    </div>
  );
}

function Body({ scope, page }: { scope: CoachScope; page: TrainPage }) {
  const total = page.counts.active + page.counts.fresh + page.counts.heldOut + page.counts.retired;
  return (
    <>
      {total === 0 ? (
        <EmptySheet
          action={
            <Button asChild variant="outline">
              <Link href={`/app/games?tc=${scope.timeClass}`}>Your games</Link>
            </Button>
          }
        >
          {NO_ITEMS_TEXT}
        </EmptySheet>
      ) : page.session.length > 0 ? (
        <TrainSession items={page.session} sources={SOURCE_LABEL} />
      ) : (
        <Summary page={page} />
      )}
      {total > 0 ? <Efficacy page={page} /> : null}
    </>
  );
}

function Summary({ page }: { page: TrainPage }) {
  const done = page.doneToday;
  return (
    <section
      aria-labelledby="train-summary"
      data-surface="sheet"
      className="flex flex-col gap-3 rounded-sheet bg-sheet px-(--space-panel-pad) py-8"
    >
      <h2 id="train-summary" className="font-serif text-2xl font-semibold text-ink">
        {done > 0 ? "Today's session is done" : "Nothing due today"}
      </h2>
      {done > 0 ? (
        <p className="m-0 text-lg text-ink [font-variant-numeric:lining-nums]">
          {count.format(page.rightToday)} of {positions(done)} right today.
        </p>
      ) : null}
      <p
        data-slot="due-tomorrow"
        className="m-0 text-lg text-ink [font-variant-numeric:lining-nums]"
      >
        {dueTomorrowText(page.dueTomorrow)}
      </p>
      <p className="m-0 max-w-prose text-base text-ink-soft">
        A position comes back after 1, 3, 7, 21 and 60 days while you keep finding it, and the next
        day when you miss it. Three sure answers a week or more apart retire it.
      </p>
    </section>
  );
}

function Efficacy({ page }: { page: TrainPage }) {
  const rows = SOURCES.flatMap((s) => page.efficacy.rows.filter((r) => r.source === s));
  return (
    <section aria-labelledby="train-efficacy" className="flex flex-col gap-5">
      <SectionHead id="train-efficacy" title="Is it working?">
        <p className="m-0 max-w-prose text-base text-ink-soft">
          How often each weakness happens per chance in your games since your first drill of it,
          against before. About {HOLDOUT_PERCENT}% of your positions are held back and never
          drilled, so drilled and undrilled patterns can be compared.
        </p>
      </SectionHead>
      <ul
        data-surface="sheet"
        className="m-0 flex list-none flex-col rounded-sheet bg-sheet p-0 py-1"
      >
        {rows.map((r) => (
          <li
            key={r.source}
            className="flex flex-col gap-1 border-b border-rule px-(--space-panel-pad) py-4 last:border-b-0"
          >
            <h3 className="m-0 font-sans text-base font-semibold text-ink">
              {SOURCE_LABEL[r.source]}
            </h3>
            <p className="m-0 text-base text-ink-soft [font-variant-numeric:lining-nums]">
              {sourceRowText(r)}
            </p>
            {r.verdict === "too_early" && r.before.chances > 0 ? (
              <p className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">
                {r.firstDrill ? "Before your first drill" : "So far"}: {rateText(r.before)}.
              </p>
            ) : null}
          </li>
        ))}
        <li className="flex flex-col gap-1 px-(--space-panel-pad) py-4">
          <h3 className="m-0 font-sans text-base font-semibold text-ink">
            Drilled against held back
          </h3>
          <p className="m-0 text-base text-ink-soft [font-variant-numeric:lining-nums]">
            {patternText(page.efficacy.patterns)}
          </p>
        </li>
      </ul>
    </section>
  );
}
