import { cn } from "@ci/ui/lib";
import { KpiStat, Stat } from "@ci/ui/stats";
import { basisLine, type EstimateView, type KpiRowView } from "./view";

export interface EstimateRowProps {
  label: string;
  /** What may honestly be shown (`estimateView`), already scaled for `format`. */
  view: EstimateView;
  format: "percent" | "decimal";
  trend?: (number | null)[];
  headingLevel?: 2 | 3 | 4;
  drawIn?: boolean;
  className?: string;
}

/**
 * One KPI row that never overstates: a value with its 80% band, a value with no band when no
 * interval can be estimated, or "Need N more games" with no number. n is always shown.
 * Lab only: Task 8 builds the product's `EstimateKpi`.
 */
export function EstimateRow({
  label,
  view,
  format,
  trend,
  headingLevel = 3,
  drawIn,
  className,
}: EstimateRowProps) {
  const Heading = `h${headingLevel}` as const;
  if (view.kind === "kpi") {
    return (
      <KpiStat
        label={label}
        value={view.value}
        format={format}
        interval={view.interval}
        basis={view.basis}
        headingLevel={headingLevel}
        {...(trend ? { trend } : {})}
        {...(drawIn ? { drawIn } : {})}
        {...(className ? { className } : {})}
      />
    );
  }
  return (
    <div className={cn("flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1", className)}>
      <div className="flex flex-col gap-0.5">
        <Heading className="text-base font-medium text-ink">{label}</Heading>
        <p className="text-sm text-ink-faint">
          {view.kind === "flat" ? basisLine(view.basis.games, view.basis.timeClass) : view.basis}
        </p>
      </div>
      {view.kind === "flat" ? (
        <div className="flex flex-col items-end gap-0.5">
          <Stat
            value={view.value}
            format={format}
            className="font-serif text-3xl leading-none font-semibold text-ink"
          />
          <p className="text-sm text-ink-soft">{view.note}</p>
        </div>
      ) : (
        <p className="font-serif text-xl text-ink-soft">{view.need}</p>
      )}
    </div>
  );
}

/** A KPI row from its view model (variants b and c). */
export function KpiRow({
  row,
  headingLevel,
  className,
}: {
  row: KpiRowView;
  headingLevel?: 2 | 3 | 4;
  className?: string;
}) {
  return (
    <EstimateRow
      label={row.label}
      view={row.view}
      format={row.format}
      {...(row.trend ? { trend: row.trend } : {})}
      {...(headingLevel ? { headingLevel } : {})}
      {...(className ? { className } : {})}
    />
  );
}
