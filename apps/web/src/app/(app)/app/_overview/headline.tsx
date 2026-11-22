import { bandText, Stat } from "@ci/ui/stats";
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

/**
 * The red-pen slot for the coaching profile's lead finding. It stays empty until the R2 profile
 * reaches the web: a "biggest leak" is never guessed from raw judgement counts (T6 ruling).
 */
export function WorkOn() {
  // Ink, not pen: red only ever marks the player's own mistakes (web polish critic P5).
  return (
    <section
      aria-labelledby="work-on-heading"
      data-slot="work-on"
      className="flex max-w-3xl flex-col gap-1 border-l-2 border-ink pl-4"
    >
      <h2 id="work-on-heading" className="text-base font-medium text-ink">
        What to work on
      </h2>
      <p className="text-sm text-ink-soft">
        Your coaching profile is still being built. When it is ready, its first finding goes here,
        with the games behind it.
      </p>
    </section>
  );
}
