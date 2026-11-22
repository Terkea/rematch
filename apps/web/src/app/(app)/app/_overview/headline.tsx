import { weaknessRateText } from "@ci/ui/profile";
import { bandText, Stat } from "@ci/ui/stats";
import type { CoachScope, LeadFinding } from "../../../../server/profile/types";
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
}: {
  lead: OverviewView["lead"];
  basisClass: string;
  reveal: boolean;
}) {
  const { accuracy, blunders } = lead;
  if (accuracy.kind === "insufficient") {
    return (
      <p className="max-w-3xl font-serif text-2xl leading-snug text-ink md:text-3xl">
        Need {accuracy.needed} more {accuracy.needed === 1 ? "game" : "games"} before an average
        accuracy means anything.
      </p>
    );
  }
  const scope = basisClass === "analysed" ? "analysed games" : `${basisClass} games`;
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

/**
 * The red-pen slot: the coaching profile's lead finding (ADR 0068), never a "biggest leak"
 * guessed from raw judgement counts. A weakness shows its label, its stored estimate with the
 * counts behind it and a link to its page; `none` says nothing stands out; `building` says the
 * profile is not ready yet.
 */
export function WorkOn({ lead }: { lead: LeadFinding }) {
  // Pen red means the player's own mistake, so only a real finding carries it.
  const finding = lead.kind === "weakness";
  return (
    <section
      aria-labelledby="work-on-heading"
      data-slot="work-on"
      className={`flex max-w-3xl flex-col gap-1 border-l-2 pl-4 ${finding ? "border-pen" : "border-rule-strong"}`}
    >
      <h2
        id="work-on-heading"
        className={`text-base font-medium ${finding ? "text-pen-text" : "text-ink"}`}
      >
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
      const q = new URLSearchParams({ account: scope.accountId, tc: scope.timeClass });
      return (
        <>
          <a
            href={`/app/weaknesses/${encodeURIComponent(w.seriesKey)}?${q.toString()}`}
            className="self-start rounded-field font-serif text-xl font-semibold text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          >
            {w.label}
          </a>
          <p className="m-0 text-sm text-ink-soft">
            {weaknessRateText(w.rate, w.evidence)} chances.
          </p>
          <p className="m-0 text-sm text-ink-faint">
            Based on {games(analysedIn(scope), `analysed ${scope.timeClass}`)}.
          </p>
        </>
      );
    }
    case "none":
      return (
        <p className="m-0 text-sm text-ink-soft">
          Nothing stands out above players at your level across{" "}
          {games(lead.analysedGames, "analysed")} yet.
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
