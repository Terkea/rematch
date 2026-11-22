import { CaretDownIcon, FunnelSimpleIcon, Icon } from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { Button, Field, Input, touchTarget } from "@ci/ui/primitives";
import Link from "next/link";
import type { ReactNode } from "react";
import type { CustomCompare, CustomRange, FilterBarModel, LinkOption } from "./model";

// The shared filter bar (T3, ADR 0083): time control, format, colour, period and the period
// comparison, as links and GET forms, so every filter works before (and without) JavaScript and
// survives a reload or a shared link. Calm and compact: each group is a `well` track like the
// segmented control, the chosen option a `sheet` chip in `ink` 600. Below `md` the whole bar
// folds into one "Filters" disclosure that names the active filters.

const chip = cn(
  "relative inline-flex h-8 shrink-0 items-center gap-1.5 rounded-pill px-3.5",
  "font-sans text-sm font-medium whitespace-nowrap text-ink-soft",
  "transition-colors duration-(--dur-quick) ease-out hover:text-ink",
  "aria-[current=true]:bg-sheet aria-[current=true]:font-semibold aria-[current=true]:text-ink",
  "aria-[current=true]:inset-ring-1 aria-[current=true]:inset-ring-rule",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
  touchTarget,
);
const track = "inline-flex h-10 w-fit items-center gap-0.5 rounded-pill bg-well p-1";
const groupLabel = "w-fit font-sans text-sm font-medium text-ink";
const summaryChip = cn(
  "relative inline-flex h-10 cursor-pointer list-none items-center gap-1.5 rounded-pill px-4",
  "font-sans text-sm font-medium text-ink-soft hover:text-ink inset-ring-1 inset-ring-rule-strong",
  "group-open/custom:bg-well group-open/custom:text-ink",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
  "[&::-webkit-details-marker]:hidden",
  touchTarget,
);

function Track({ options, label }: { options: LinkOption[]; label: string }) {
  return (
    <div className="max-w-full overflow-x-auto pointer-coarse:-my-1.5 pointer-coarse:py-1.5">
      <ul aria-label={label} className={cn(track, "m-0 list-none")}>
        {options.map((o) => (
          <li key={o.href} className="flex">
            <Link
              href={o.href}
              scroll={false}
              aria-current={o.current ? "true" : undefined}
              className={chip}
            >
              {o.label}
              {o.count !== undefined ? (
                <span className="font-normal tabular-nums text-ink-faint">{o.count}</span>
              ) : null}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Group({ id, label, children }: { id: string; label: string; children: ReactNode }) {
  return (
    <div role="group" aria-labelledby={id} className="flex min-w-0 flex-col gap-1.5">
      <span id={id} className={groupLabel}>
        {label}
      </span>
      {children}
    </div>
  );
}

const Hidden = ({ pairs }: { pairs: [string, string][] }) =>
  pairs.map(([k, v]) => <input key={`${k}=${v}`} type="hidden" name={k} value={v} />);

function DateInput({ label, name, value }: { label: string; name: string; value: string | null }) {
  return (
    <Field label={label} className="w-40">
      <Input type="date" name={name} defaultValue={value ?? ""} min="2000-01-01" max="2100-12-31" />
    </Field>
  );
}

function CustomRangeForm({ custom }: { custom: CustomRange }) {
  return (
    <details className="group/custom" {...(custom.open ? { open: true } : {})}>
      <summary className={summaryChip}>
        Custom
        <Icon icon={CaretDownIcon} size={16} className="group-open/custom:rotate-180" />
      </summary>
      <form method="get" action={custom.action} className="flex flex-wrap items-end gap-3 pt-3">
        <Hidden pairs={custom.hidden} />
        <DateInput label="From" name="from" value={custom.from} />
        <DateInput label="To" name="to" value={custom.to} />
        <Button type="submit" variant="outline">
          Show these dates
        </Button>
      </form>
    </details>
  );
}

function CustomCompareForm({ custom }: { custom: CustomCompare }) {
  return (
    <details className="group/custom" {...(custom.open ? { open: true } : {})}>
      <summary className={summaryChip}>
        Custom
        <Icon icon={CaretDownIcon} size={16} className="group-open/custom:rotate-180" />
      </summary>
      <form method="get" action={custom.action} className="flex flex-wrap items-end gap-4 pt-3">
        <Hidden pairs={custom.hidden} />
        <fieldset className="m-0 flex flex-wrap items-end gap-3 border-0 p-0">
          <legend className="mb-1.5 text-sm text-ink-soft">Period A, the earlier one</legend>
          <DateInput label="A from" name="a_from" value={custom.a.from} />
          <DateInput label="A to" name="a_to" value={custom.a.to} />
        </fieldset>
        <fieldset className="m-0 flex flex-wrap items-end gap-3 border-0 p-0">
          <legend className="mb-1.5 text-sm text-ink-soft">Period B, the later one</legend>
          <DateInput label="B from" name="b_from" value={custom.b.from} />
          <DateInput label="B to" name="b_to" value={custom.b.to} />
        </fieldset>
        <Button type="submit" variant="outline">
          Compare these periods
        </Button>
      </form>
    </details>
  );
}

function Body({
  model: m,
  idPrefix,
  colorNote,
}: {
  model: FilterBarModel;
  idPrefix: string;
  colorNote: string;
}) {
  const id = (s: string) => `${idPrefix}-${s}`;
  return (
    <div className="flex flex-col gap-4 md:flex-row md:flex-wrap md:items-start md:gap-x-8 md:gap-y-4">
      <Group id={id("tc")} label="Time control">
        <Track label="Time control" options={m.timeClass} />
      </Group>
      {m.format ? (
        <Group id={id("fmt")} label="Format">
          <Track label="Format" options={m.format} />
        </Group>
      ) : null}
      {m.color ? (
        <Group id={id("color")} label="Colour">
          <Track label="Colour" options={m.color} />
        </Group>
      ) : m.colorComing ? (
        <Group id={id("color")} label="Colour">
          <p
            data-slot="colour-coming"
            className="m-0 flex min-h-10 max-w-xs items-center text-sm text-ink-soft"
          >
            {colorNote}
          </p>
        </Group>
      ) : null}
      {m.period ? (
        <Group id={id("period")} label="Period">
          <div className="flex flex-wrap items-start gap-2">
            <Track label="Period" options={m.period.options} />
            <CustomRangeForm custom={m.period.custom} />
          </div>
        </Group>
      ) : null}
      {m.compare ? (
        <Group id={id("cmp")} label="Compare periods">
          <div className="flex flex-wrap items-start gap-2">
            {m.compare.on ? (
              <>
                <Track label="Periods to compare" options={m.compare.options} />
                <CustomCompareForm custom={m.compare.custom} />
              </>
            ) : null}
            <Link
              href={m.compare.toggleHref}
              scroll={false}
              className={cn(summaryChip, m.compare.on ? "" : "text-ink")}
            >
              {m.compare.on ? "Stop comparing" : "Compare periods"}
            </Link>
          </div>
          {m.periodReplaced ? (
            <p className="m-0 text-sm text-ink-faint">The two periods replace the date range.</p>
          ) : null}
        </Group>
      ) : null}
    </div>
  );
}

/**
 * The bar. `colorNote` is the honest line a page shows where its stored data has no colour split
 * yet (ADR 0082). From `md` the groups sit in one wrapping row; below it one disclosure holds them.
 */
export function FilterBar({
  model,
  colorNote = "Colour split for weaknesses is coming. These numbers cover both colours.",
}: {
  model: FilterBarModel;
  colorNote?: string;
}) {
  return (
    <section aria-label="Filters" data-filter-bar="">
      <details className="group md:hidden">
        <summary
          className={cn(
            "relative flex min-h-11 cursor-pointer list-none items-center gap-3 rounded-field px-3 py-2",
            "inset-ring-1 inset-ring-rule-strong hover:bg-(--surface-hover)",
            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
            "[&::-webkit-details-marker]:hidden",
          )}
        >
          <Icon icon={FunnelSimpleIcon} size={20} className="shrink-0 text-ink" />
          <span className="shrink-0 font-sans text-base font-medium text-ink">
            Filters{m(model)}
          </span>
          <span className="min-w-0 flex-1 truncate text-sm text-ink-soft">{model.summary}</span>
          <Icon
            icon={CaretDownIcon}
            size={20}
            className="shrink-0 text-ink-soft group-open:rotate-180"
          />
        </summary>
        <div className="pt-4">
          <Body model={model} idPrefix="fb-m" colorNote={colorNote} />
        </div>
      </details>
      <div className="hidden md:block">
        <Body model={model} idPrefix="fb-d" colorNote={colorNote} />
      </div>
    </section>
  );
}

const m = (model: FilterBarModel) => (model.active > 0 ? ` (${model.active})` : "");
