import { basisText, KpiStat, Stat } from "@ci/ui/stats";
import type { Estimate } from "../server/stats";

export interface EstimateKpiProps {
  label: string;
  estimate: Estimate;
  /** `percent` takes 0 to 100 (accuracy, score in percent); `decimal` is per game. */
  format: "percent" | "decimal";
  /** The time class name, or "analysed" for all games. */
  basisClass: string;
  /** One value per week, oldest first, in the estimate's units; null is a week with no games. */
  trend?: (number | null)[];
  /** The games behind each trend point: the sparkline's ends then name their week and count. */
  trendGames?: number[];
  headingLevel?: 2 | 3;
  /** The games the section already states: a row on the same basis keeps it for screen readers. */
  sectionGames?: number;
  /** Which way is better, written under the sparkline with its first and last values. */
  better?: "higher" | "lower";
  /** Roll the number up on a first view (the report reveal). */
  rollIn?: boolean;
}

/**
 * A KPI that never overstates (spec 12a): the value with its 80% band in words; the value alone
 * when no interval can be estimated (no spread; never widened); or, below the minimum, "Need N
 * more games" with its basis and no number. The overview draws no ruler: it has no honest
 * reference range for a per-game rate (ADR 0055). Safe in a server component.
 */
export function EstimateKpi({
  label,
  estimate,
  format,
  basisClass,
  trend,
  trendGames,
  headingLevel = 3,
  sectionGames,
  better,
  rollIn,
}: EstimateKpiProps) {
  const Heading = `h${headingLevel}` as const;
  const k = format === "percent" ? 100 : 1;
  if (estimate.kind === "insufficient") {
    const need = `Need ${estimate.needed} more ${estimate.needed === 1 ? "game" : "games"}`;
    // On the section's own basis, the section states the basis and the shortfall once; the row
    // keeps both for screen readers and shows a dash, so a sparse page doesn't repeat itself.
    const stated = sectionGames !== undefined && sectionGames === estimate.n;
    return (
      <div
        data-kpi="insufficient"
        className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1"
      >
        <div className="flex flex-col gap-0.5">
          <Heading className="text-base font-medium text-ink">{label}</Heading>
          <p className={stated ? "sr-only" : "text-sm text-ink-soft"}>
            {basisText({ games: estimate.n, timeClass: basisClass })}
          </p>
        </div>
        {stated ? (
          <p className="font-serif text-xl text-ink-faint">
            <span aria-hidden="true">–</span>
            <span className="sr-only">{need}</span>
          </p>
        ) : (
          <p className="font-serif text-xl text-ink-soft">{need}</p>
        )}
      </div>
    );
  }
  const basis = { games: estimate.n, timeClass: basisClass };
  const basisHidden = sectionGames !== undefined && sectionGames === estimate.n;
  if (estimate.interval === null) {
    return (
      <div
        data-kpi="flat"
        className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1"
      >
        <div className="flex flex-col gap-0.5">
          <Heading className="text-base font-medium text-ink">{label}</Heading>
          <p className={basisHidden ? "sr-only" : "text-sm text-ink-soft"}>{basisText(basis)}</p>
        </div>
        <div className="flex flex-col items-end gap-0.5">
          <Stat
            value={estimate.value / k}
            format={format}
            className="font-serif text-3xl leading-none font-semibold text-ink"
          />
          <p className="text-sm text-ink-soft">
            Every game gave the same value, so there is no interval
          </p>
        </div>
      </div>
    );
  }
  const scaled = trend?.map((v) => (v === null ? null : v / k));
  return (
    <KpiStat
      label={label}
      value={estimate.value / k}
      format={format}
      interval={{
        low: estimate.interval.low / k,
        high: estimate.interval.high / k,
        level: estimate.interval.level,
      }}
      basis={basis}
      headingLevel={headingLevel}
      scale={false}
      {...(basisHidden ? { basisHidden } : {})}
      {...(scaled ? { trend: scaled, trendEnds: true } : {})}
      {...(scaled && trendGames ? { trendGames } : {})}
      {...(better ? { better } : {})}
      {...(rollIn ? { rollIn, rollDelay: 200 } : {})}
    />
  );
}
