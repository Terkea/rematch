import { NO_BENCHMARK_TEXT, weaknessRateText } from "@ci/ui/profile";
import { bandText, Stat } from "@ci/ui/stats";
import { type HeadlinePeer, headlineMeaning } from "../../../../lib/explain";
import type { CoachScope, LeadFinding } from "../../../../server/profile/types";
import { ESTIMATE_WINDOW_TEXT } from "../../../../server/profile/window";
import type { Estimate } from "../../../../server/stats";
import type { OverviewView } from "./view";

/** The band in words for the lead, or why there is none. Nothing below the minimum. */
function band(name: string, e: Estimate, format: "percent" | "decimal"): string | null {
  if (e.kind === "insufficient") return null;
  if (e.interval === null)
    return `${name}: every game gave the same value, so there is no interval.`;
  const k = format === "percent" ? 100 : 1;
  const text = bandText(
    { low: e.interval.low / k, high: e.interval.high / k, level: e.interval.level },
    format,
  );
  return `${name} ${text}.`;
}

/**
 * The headline numbers as one sentence (variant b), with n stated once and both bands written out
 * under it. On a phone the sentence is shorter, at a smaller display size. Below the minimum it
 * says how many more games it needs and shows no number.
 */
export function Headline({
  lead,
  basisClass,
  reveal,
  peer = null,
}: {
  lead: OverviewView["lead"];
  basisClass: string;
  reveal: boolean;
  /** The players faced (Q1): the meaning line compares with them. */
  peer?: HeadlinePeer | null;
}) {
  const { accuracy, blunders } = lead;
  const scope = basisClass === "analysed" ? "analysed games" : `${basisClass} games`;
  const meaning = (
    <p data-slot="meaning" className="m-0 max-w-prose text-base text-ink">
      {headlineMeaning({ accuracy, blunders, games: scope, peer })}
    </p>
  );
  if (accuracy.kind === "insufficient") {
    return (
      <div className="flex max-w-3xl flex-col gap-2">
        <p className="font-serif text-2xl leading-snug text-ink md:text-3xl">
          Need {accuracy.needed} more {accuracy.needed === 1 ? "game" : "games"} before an average
          accuracy means anything.
        </p>
        {meaning}
      </div>
    );
  }
  const acc = (
    <Stat
      value={accuracy.value / 100}
      format="percent"
      {...(reveal ? { rollIn: true, rollDelay: 200 } : {})}
    />
  );
  const bl =
    blunders.kind === "insufficient" ? null : (
      <Stat
        value={blunders.value}
        format="decimal"
        {...(reveal ? { rollIn: true, rollDelay: 200 } : {})}
      />
    );
  const bands = [band("Accuracy", accuracy, "percent"), band("Blunders", blunders, "decimal")]
    .filter((b): b is string => b !== null)
    .join(" ");
  return (
    <div className="flex max-w-3xl flex-col gap-2">
      <p className="font-serif text-2xl leading-snug text-ink sm:hidden">
        {acc} accuracy{bl ? <>, {bl} blunders a game.</> : "."}
      </p>
      <p className="hidden font-serif text-3xl leading-tight text-ink sm:block lg:text-4xl">
        Across {accuracy.n} {scope} your accuracy averages {acc}
        {bl ? <>, with {bl} blunders a game.</> : "."}
      </p>
      <p className="text-sm text-ink-soft">
        <span className="sm:hidden">
          {accuracy.n} {scope}.{" "}
        </span>
        {bands}
      </p>
      {meaning}
    </div>
  );
}

const count = new Intl.NumberFormat("en-GB");
/** "214 analysed rapid games" (`kind` between the count and the noun). */
const games = (n: number, kind: string) =>
  `${count.format(n)} ${kind} ${n === 1 ? "game" : "games"}`;

function analysedIn(scope: CoachScope): number {
  return (
    scope.options.find((o) => o.accountId === scope.accountId && o.timeClass === scope.timeClass)
      ?.analysedGames ?? 0
  );
}

const PLATFORM = { lichess: "lichess", chesscom: "chess.com" } as const;

/**
 * "214 analysed rapid games on lichess": the lead's own scope, which is the default coaching
 * scope and not the overview's (an "All" overview must not borrow blitz's count, bug hunt M6).
 */
const scopeGames = (n: number, scope: CoachScope) =>
  `${games(n, `analysed ${scope.timeClass}`)} on ${PLATFORM[scope.platform]}`;

const scopeQuery = (scope: CoachScope) =>
  new URLSearchParams({ account: scope.accountId, tc: scope.timeClass }).toString();

/**
 * The red-pen slot: the coaching profile's lead finding (ADR 0068), never a "biggest leak"
 * guessed from raw judgement counts. A weakness shows its label, its stored estimate with the
 * counts behind it and a link to its page; `none` says nothing stands out; `building` says the
 * profile is not ready yet.
 */
export function WorkOn({ lead }: { lead: LeadFinding }) {
  // A plain sheet: no coloured border and no red heading (B10). Pen red stays for the player's
  // own moves.
  return (
    <section
      aria-labelledby="work-on-heading"
      data-slot="work-on"
      data-surface="sheet"
      className="flex max-w-3xl flex-col gap-1 rounded-sheet bg-sheet px-(--space-panel-pad) py-4"
    >
      <h2 id="work-on-heading" className="text-base font-medium text-ink">
        What to work on
      </h2>
      <WorkOnBody lead={lead} />
    </section>
  );
}

function WorkOnBody({ lead }: { lead: LeadFinding }) {
  switch (lead.kind) {
    case "weakness": {
      const { weakness: w, scope } = lead;
      return (
        <>
          <a
            href={`/app/weaknesses/${encodeURIComponent(w.seriesKey)}?${scopeQuery(scope)}`}
            className="self-start rounded-field font-serif text-xl font-semibold text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          >
            {w.label}
          </a>
          <p className="m-0 text-sm text-ink-soft">
            {weaknessRateText(w.rate, w.evidence, ESTIMATE_WINDOW_TEXT)}.
          </p>
          <p className="m-0 text-sm text-ink-faint">
            Based on {scopeGames(analysedIn(scope), scope)}.
          </p>
        </>
      );
    }
    case "none":
      return lead.compared ? (
        <p className="m-0 text-sm text-ink-soft">
          Nothing stands out above the players you are compared with across{" "}
          {scopeGames(lead.analysedGames, lead.scope)} yet.
        </p>
      ) : (
        // No benchmark: nothing was compared, so "nothing stands out" would overclaim (I1).
        <p className="m-0 text-sm text-ink-soft">
          {NO_BENCHMARK_TEXT}. Based on {scopeGames(lead.analysedGames, lead.scope)},{" "}
          <a
            href={`/app/weaknesses?${scopeQuery(lead.scope)}`}
            className="rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          >
            your weaknesses
          </a>{" "}
          show their own rates until then.
        </p>
      );
    default:
      return (
        <p className="m-0 text-sm text-ink-soft">
          Your coaching profile is still being built. When it is ready, its first finding goes here,
          with the games behind it.
        </p>
      );
  }
}
