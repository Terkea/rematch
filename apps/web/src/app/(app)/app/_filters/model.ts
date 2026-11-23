import { TIME_CLASS_LABEL, TIME_CLASS_ORDER } from "../../../../lib/time-class";
import type { FormatOption } from "../../../../server/filters/formats";
import {
  type CompareMode,
  type DayRange,
  type FilterContext,
  type Params,
  parseFilters,
  type StatsFilters,
  serializeFilters,
} from "../../../../server/filters/params";
import {
  DEFAULT_PRESET,
  type FixedPreset,
  presetOf,
  presetRange,
} from "../../../../server/filters/presets";
import { KPI_MIN_GAMES } from "../../../../server/stats/constants";
import type { TimeClass } from "../../../../server/games/types";

// The shared filter bar's view model (T3, ADR 0083): every control is a link or a GET form built
// from `serializeFilters`, so the bar works without JavaScript and a link is always canonical.
// A page says which controls its data supports; a control the data cannot honour is absent, or
// (colour on the stored weakness estimates) an honest "coming" note.

export type BarConfig = {
  /** The page's path, without a query. */
  basePath: string;
  /** The classes with games, with their counts; shown rapid first (I8). */
  timeClasses: readonly { tc: TimeClass; games: number }[];
  /** The page reads `tc=all` as every class (the overview and the games page). */
  allTimeClasses?: boolean;
  /** The current class's format options (`listFormats`). */
  formats: readonly FormatOption[];
  /** False where the page's data has no format dimension. */
  formatSupported?: boolean;
  /** "coming": the page's stored data has no colour split yet (ADR 0082). */
  color: "on" | "coming" | "off";
  dates: boolean;
  compare: boolean;
  /** Params of the page's own contract kept on every link (the coach scope's `account`). */
  keep?: Readonly<Record<string, string>>;
  now: Date;
  /** What a URL without dates shows: the last 90 days, or every game (the games list). */
  defaultPeriod?: "90d" | "all";
  /** The class's games in the default 90 days, for the thin-window note; unknown when absent. */
  windowGames?: number | null;
};

export type LinkOption = { label: string; href: string; current: boolean; count?: number };

export type CustomRange = {
  open: boolean;
  action: string;
  from: string | null;
  to: string | null;
  /** Every other param, as hidden inputs. */
  hidden: [string, string][];
};

export type CustomCompare = {
  open: boolean;
  action: string;
  a: DayRange;
  b: DayRange;
  hidden: [string, string][];
};

export type FilterBarModel = {
  timeClass: LinkOption[];
  format: LinkOption[] | null;
  color: LinkOption[] | null;
  /** The colour filter is asked for, but this page's data has no colour split yet. */
  colorComing: boolean;
  period: { options: LinkOption[]; custom: CustomRange } | null;
  /** Comparing replaces the date range, so the presets are hidden. */
  periodReplaced: boolean;
  compare: {
    on: boolean;
    toggleHref: string;
    options: LinkOption[];
    custom: CustomCompare;
  } | null;
  /** Active filters beyond the time class. */
  active: number;
  /** The filters in words ("Rapid, 10+0, Black, last 30 days"). */
  summary: string;
  /** The summary without the time control, for a bar shown without it (ADR 0092). */
  filtersSummary: string;
  /**
   * The default 90 days hold fewer games than the numbers need (`KPI_MIN_GAMES`): the line that
   * says so and the one link to all time (ADR 0083 Amendment 2).
   */
  thin: { text: string; allTimeHref: string } | null;
};

const ALL = "all";
const PRESET_LABEL: Record<FixedPreset, string> = {
  "30d": "30 days",
  "90d": "90 days",
  "6m": "6 months",
  "12m": "12 months",
  all: "All time",
};
const PRESET_WORDS: Record<Exclude<FixedPreset, "all">, string> = {
  "30d": "last 30 days",
  "90d": "last 90 days",
  "6m": "last 6 months",
  "12m": "last 12 months",
};
const FIXED: FixedPreset[] = ["30d", "90d", "6m", "12m", "all"];
const COMPARE_LABEL: Record<Exclude<CompareMode, "custom">, string> = {
  month: "This month vs last",
  "30d": "Last 30 days vs the 30 before",
  "50g": "Last 50 games vs the 50 before",
};
const COMPARE_WORDS: Record<CompareMode, string> = {
  month: "this month vs last",
  "30d": "last 30 days vs the 30 before",
  "50g": "last 50 games vs the 50 before",
  custom: "two custom periods",
};
const COLOR_LABEL = { both: "Both", white: "White", black: "Black" } as const;

function context(cfg: BarConfig): FilterContext {
  return {
    available: cfg.timeClasses.map((c) => c.tc),
    ...(cfg.formatSupported === false ? {} : { formats: cfg.formats.map((f) => f.key) }),
    now: cfg.now,
    ...(cfg.defaultPeriod ? { defaultPeriod: cfg.defaultPeriod } : {}),
  };
}

const wantsAll = (params: Params) => {
  const v = params instanceof URLSearchParams ? params.getAll("tc") : [params.tc].flat();
  return v.length === 1 && v[0] === ALL;
};

/** The filters of the URL; with `allTimeClasses`, `tc=all` is a null class (every class). */
export function parseBarFilters(params: Params, cfg: BarConfig): StatsFilters {
  const f = parseFilters(params, context(cfg));
  const all = cfg.allTimeClasses === true && wantsAll(params) && cfg.timeClasses.length > 0;
  const out = all ? { ...f, tc: null, fmt: null } : f;
  return cfg.formatSupported === false ? { ...out, fmt: null } : out;
}

/** The canonical query of `f` (`tc=all` first for every class), then the page's own params. */
export function barQuery(f: StatsFilters, cfg: BarConfig): URLSearchParams {
  const q = serializeFilters(f, context(cfg));
  const out = new URLSearchParams();
  if (f.tc === null && cfg.allTimeClasses && cfg.timeClasses.length > 0) out.set("tc", ALL);
  for (const [k, v] of q) out.append(k, v);
  for (const [k, v] of Object.entries(cfg.keep ?? {})) out.set(k, v);
  return out;
}

export function barHref(f: StatsFilters, cfg: BarConfig): string {
  const q = barQuery(f, cfg).toString();
  return q ? `${cfg.basePath}?${q}` : cfg.basePath;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
/** "1 Oct 2026"; without the year when `year` is false. */
export function dayWords(day: string, year = true): string {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  return `${d} ${MONTHS[m - 1] ?? ""}${year ? ` ${y}` : ""}`;
}

/** "1 Oct to 31 Oct 2026", "from 1 Oct 2026", "up to 31 Oct 2026", or "all time". */
export function rangeWords(r: DayRange): string {
  if (r.from && r.to) {
    const sameYear = r.from.slice(0, 4) === r.to.slice(0, 4);
    return `${dayWords(r.from, !sameYear)} to ${dayWords(r.to)}`;
  }
  if (r.from) return `from ${dayWords(r.from)}`;
  if (r.to) return `up to ${dayWords(r.to)}`;
  return "all time";
}

const hiddenOf = (q: URLSearchParams, drop: readonly string[]): [string, string][] =>
  [...q.entries()].filter(([k]) => !drop.includes(k));

export function filterBarModel(f: StatsFilters, cfg: BarConfig): FilterBarModel {
  const href = (patch: Partial<StatsFilters>) => barHref({ ...f, ...patch }, cfg);
  const counts = new Map(cfg.timeClasses.map((c) => [c.tc, c.games]));
  const ordered = TIME_CLASS_ORDER.filter((tc) => counts.has(tc));

  const timeClass: LinkOption[] = ordered.map((tc) => ({
    label: TIME_CLASS_LABEL[tc],
    count: counts.get(tc) ?? 0,
    href: href({ tc, fmt: null }),
    current: f.tc === tc,
  }));
  if (cfg.allTimeClasses && ordered.length > 0) {
    timeClass.push({
      label: "All",
      count: cfg.timeClasses.reduce((s, c) => s + c.games, 0),
      href: href({ tc: null, fmt: null }),
      current: f.tc === null,
    });
  }

  const format =
    cfg.formatSupported === false || f.tc === null || f.tc === "daily" || cfg.formats.length === 0
      ? null
      : [
          { label: "All formats", href: href({ fmt: null }), current: f.fmt === null },
          ...cfg.formats.map((o) => ({
            label: o.label,
            href: href({ fmt: o.key }),
            current: f.fmt === o.key,
          })),
        ];

  const color =
    cfg.color === "on"
      ? (["both", "white", "black"] as const).map((c) => ({
          label: COLOR_LABEL[c],
          href: href({ color: c }),
          current: f.color === c,
        }))
      : null;

  const range = { from: f.from, to: f.to };
  const preset = presetOf(range, cfg.now);
  const defaultPreset: FixedPreset = cfg.defaultPeriod === "all" ? "all" : DEFAULT_PRESET;
  const comparing = f.cmp !== null;
  const period =
    cfg.dates && !(cfg.compare && comparing)
      ? {
          options: FIXED.map((p) => ({
            label: PRESET_LABEL[p],
            href: href(presetRange(p, cfg.now)),
            current: preset === p,
          })),
          custom: {
            open: preset === "custom",
            action: cfg.basePath,
            from: f.from,
            to: f.to,
            hidden: hiddenOf(barQuery({ ...f, from: null, to: null }, cfg), ["from", "to"]),
          },
        }
      : null;

  const compare = cfg.compare
    ? {
        on: comparing,
        toggleHref: comparing
          ? href({ cmp: null, a: null, b: null })
          : href({ cmp: "month", a: null, b: null }),
        options: comparing
          ? (["month", "30d", "50g"] as const).map((c) => ({
              label: COMPARE_LABEL[c],
              href: href({ cmp: c, a: null, b: null }),
              current: f.cmp === c,
            }))
          : [],
        custom: {
          open: f.cmp === "custom",
          action: cfg.basePath,
          a: f.a ?? { from: null, to: null },
          b: f.b ?? { from: null, to: null },
          hidden: [
            ...hiddenOf(barQuery({ ...f, cmp: null, a: null, b: null }, cfg), []),
            ["cmp", "custom"] as [string, string],
          ],
        },
      }
    : null;

  const words: string[] = [f.tc === null ? "All time controls" : TIME_CLASS_LABEL[f.tc]];
  let active = 0;
  const fmtLabel = f.fmt ? cfg.formats.find((o) => o.key === f.fmt)?.label : undefined;
  if (fmtLabel && format) {
    words.push(fmtLabel);
    active++;
  }
  if (f.color !== "both" && cfg.color !== "off") {
    words.push(COLOR_LABEL[f.color]);
    active++;
  }
  if (cfg.compare && comparing && f.cmp) {
    words.push(COMPARE_WORDS[f.cmp]);
    active++;
  } else if (cfg.dates && (f.from || f.to || preset !== defaultPreset)) {
    words.push(preset === "custom" || preset === "all" ? rangeWords(range) : PRESET_WORDS[preset]);
    if (preset !== defaultPreset) active++;
  }

  const games = cfg.windowGames;
  const thin =
    cfg.dates &&
    !(cfg.compare && comparing) &&
    preset === DEFAULT_PRESET &&
    defaultPreset === DEFAULT_PRESET &&
    typeof games === "number" &&
    games < KPI_MIN_GAMES
      ? {
          text: `${games} ${games === 1 ? "game" : "games"} in the last 90 days. Most numbers here need at least ${KPI_MIN_GAMES}.`,
          allTimeHref: href(presetRange("all", cfg.now)),
        }
      : null;

  return {
    timeClass,
    format,
    color,
    colorComing: cfg.color === "coming",
    period,
    periodReplaced: cfg.dates && cfg.compare && comparing,
    compare,
    active,
    summary: words.join(", "),
    filtersSummary: words.slice(1).join(", "),
    thin,
  };
}
