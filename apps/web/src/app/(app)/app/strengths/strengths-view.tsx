import { Button } from "@ci/ui/primitives";
import { dayText, exemplarMoveText, fenMove, formatKpiValue } from "@ci/ui/profile";
import Link from "next/link";
import type { ReactNode } from "react";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import { GamePosition } from "../../../../components/game-position";
import type { CoachScope, DifficultyStatus } from "../../../../server/profile";
import {
  BENCHMARK_BIAS_TEXT,
  type Benchmark,
  benchmarkGroupLabel,
  benchmarkLabel,
} from "../../../../server/profile/benchmark-text";
import { AREA_GLOSSARY } from "../../../../server/profile/glossary";
import {
  BALANCED_MAX,
  BALANCED_MIN,
  type BandClaim,
  CONFIRM_P_CHANGE,
  CONFIRM_REL_CHANGE,
  CRITICAL_GAP,
  type HardSpotCandidate,
  type ImprovingWeakness,
  MIN_OPENING_GAMES,
  type OwnClaim,
  type OwnTrends,
  type PeerClaim,
  PROMISING_P,
  type PromisingOpening,
  RECENT_GAMES,
  type StrengthsModel,
  type Verdict,
  type WorkingOpening,
} from "../../../../server/profile/strengths";
import { EmptySheet } from "../../_shell/page-heading";
import { SectionHead } from "../_overview/section-head";

// `/app/strengths`: the good things in the scope's games and what to keep doing. Everything here
// is worded from the read model's numbers (`server/profile/strengths.ts`), which has already
// refused every claim without its minimum evidence; this file only says what passed, with its n.

export const EMPTY_TEXT =
  "No analysed games yet. Your strengths appear once some of your games are analysed.";

/** The report's verdict words (`report/_sections/openings.tsx` `VERDICT_TEXT`; a test pins them). */
export const VERDICT_TEXT: Record<Verdict, string> = {
  keep: "Keep",
  fix: "Fix",
  learn: "Learn deeper",
  replace: "Replace",
};

/** The section title for the fallback, which never claims the moves were hard (N11). */
export const HARD_TITLE = "Your best moves in hard spots";
export const CRITICAL_TITLE = "Critical moments you got right";

export const OWN_BASELINE_TEXT =
  "Compared with your own earlier games, since there's no benchmark for your level yet.";

const count = new Intl.NumberFormat("en-GB");
const MINUS = "−";
const TIME_CLASS = {
  bullet: "bullet",
  blitz: "blitz",
  rapid: "rapid",
  classical: "classical",
  daily: "daily",
} as const;
const PLATFORM = { lichess: "lichess", chesscom: "chess.com" } as const;
const fin = (v: number | null | undefined): v is number =>
  typeof v === "number" && Number.isFinite(v);
const iso = (d: Date) => d.toISOString().slice(0, 10);
const plural = (n: number, one: string, many: string) =>
  `${count.format(n)} ${n === 1 ? one : many}`;
const pct0 = (share: number) => `${Math.round(Math.min(1, Math.max(0, share)) * 100)}%`;
const pct1 = (share: number) => `${(share * 100).toFixed(1)}%`;

/** "Based on 214 analysed rapid games on chess.com." */
export function basisText(scope: CoachScope): string {
  const o = scope.options.find(
    (x) => x.accountId === scope.accountId && x.timeClass === scope.timeClass,
  );
  const n = Math.max(0, Math.trunc(o?.analysedGames ?? 0));
  return `Based on ${plural(n, `analysed ${TIME_CLASS[scope.timeClass]} game`, `analysed ${TIME_CLASS[scope.timeClass]} games`)} on ${PLATFORM[scope.platform]}.`;
}

const scopeQuery = (scope: CoachScope) =>
  new URLSearchParams({ account: scope.accountId, tc: scope.timeClass }).toString();

/** What a metric's per-game value counts, for "over your last 40 games". */
const N_UNIT: Record<string, [string, string]> = {
  conversion_rate: ["game with a winning position", "games with a winning position"],
  resilience_rate: ["game with a lost position", "games with a lost position"],
};
const nOf = (metric: string, n: number) => {
  const [one, many] = N_UNIT[metric] ?? ["game", "games"];
  return plural(n, one, many);
};

/** A signed difference in the metric's own unit: "+8.2 points", "−0.40 a game". */
export function diffText(v: number, unit: string | null): string {
  if (!fin(v)) return "not measured";
  const sign = v < 0 ? MINUS : "+";
  const a = Math.abs(v);
  switch (unit) {
    case "pct":
      return `${sign}${a.toFixed(1)} ${a.toFixed(1) === "1.0" ? "point" : "points"}`;
    case "rate":
      return `${sign}${(a * 100).toFixed(1)} percentage points`;
    case "per_game":
      return `${sign}${a.toFixed(2)} a game`;
    default:
      return `${sign}${a.toFixed(2)}`;
  }
}

const value = (v: number, unit: string | null) => formatKpiValue(v, unit).replace("-", MINUS);
/** A row's headline figure, with its unit: "76.1%", "0.80 a game". */
const figureOf = (v: number, unit: string | null) =>
  unit === "per_game" && fin(v) ? `${value(v, unit)} a game` : value(v, unit);
const range = (low: number, high: number, unit: string | null) =>
  `${value(low, unit)} to ${value(high, unit)}`;

/** "80% range 72.8% to 75.6%, over your last 40 games. Players in your rating band: 70.0%." */
export function bandClaimText(c: BandClaim): string {
  return `80% range ${range(c.low, c.high, c.unit)}, over your last ${nOf(c.metric, c.n)}. Players in your rating band: ${value(c.median, c.unit)}.`;
}

/**
 * "80% range 74.1% to 78.3%, over your last 50 games. Players you've faced, rated about 650–950:
 * 71.4% (the middle of 25 players' averages)."
 */
export function peerClaimText(c: PeerClaim, b: Benchmark): string {
  const group = benchmarkGroupLabel(b) ?? "Players you've faced";
  const basis = c.pooled
    ? c.chances != null
      ? `pooled over ${count.format(Math.max(0, Math.trunc(c.chances)))} of their chances`
      : "pooled over their games"
    : `the middle of ${plural(c.players, "player's average", "players' averages")}`;
  return `80% range ${range(c.low, c.high, c.unit)}, over your last ${nOf(c.metric, c.n)}. ${group}: ${value(c.peer, c.unit)} (${basis}).`;
}

/** "76.1% over your last 50 games, against 65.0% over 40 earlier games. …" */
export function ownClaimText(c: OwnClaim): string {
  return `${value(c.recent.value, c.unit)} over your last ${nOf(c.metric, c.recent.n)}, against ${value(c.earlier.value, c.unit)} over ${nOf(c.metric, c.earlier.n).replace(/^(\S+) /, "$1 earlier ")}. The change's 80% range is ${diffText(c.diff.low, c.unit)} to ${diffText(c.diff.high, c.unit)}.`;
}

/** "4.0% of chances before, 2.6% after, detected improving on 14 Oct 2026 (86% likely). …" */
export function weaknessChangeText(w: ImprovingWeakness): string {
  const on = w.at ? ` on ${dayText(iso(w.at))}` : "";
  const since = w.afterCounts
    ? ` Since then: ${plural(w.afterCounts.events, "time", "times")} in ${plural(w.afterCounts.chances, "chance", "chances")}.`
    : "";
  return `${pct1(w.before)} of chances before, ${pct1(w.after)} after, detected ${w.state}${on} (${pct0(w.pChange)} likely a real change).${since}`;
}

/** "Scored 64% in 14 games (80% range 52% to 75%); your rating predicted 51%." */
export function openingText(o: WorkingOpening): string {
  return `Scored ${pct0(o.score)} in ${plural(o.n, "game", "games")} (80% range ${pct0(o.low)} to ${pct0(o.high)}); your rating predicted ${pct0(o.expected)}.`;
}

/** "Scored 67% in 12 games; your rating predicted 52%. 86% likely above it, not yet certain." */
export function promisingText(o: PromisingOpening): string {
  return `Scored ${pct0(o.score)} in ${plural(o.n, "game", "games")}; your rating predicted ${pct0(o.expected)}. ${pct0(Math.min(o.pAbove, 0.99))} likely above it, not yet certain.`;
}

/** The note under a board: why it counts, then the win chance and the clock. */
export function spotText(s: HardSpotCandidate, mode: "maia" | "critical"): string {
  const why =
    mode === "maia" && fin(s.pBest)
      ? `Maia expects about ${pct0(s.pBest)} of players at your level to find it.`
      : fin(s.multipvGap)
        ? `The engine's next choice was ${Math.round(s.multipvGap)} win% points worse.`
        : "";
  const pc = (v: number) => `${Math.round(Math.min(100, Math.max(0, v)))}%`;
  const clock = fin(s.timeSpent) ? ` You took ${Math.max(0, Math.round(s.timeSpent))} s.` : "";
  return `${why} Win chance ${pc(s.winBefore)} to ${pc(s.winAfter)}.${clock}`.trim();
}

// ---- Pieces ----

function Section({
  id,
  title,
  lede,
  children,
}: {
  id: string;
  title: string;
  lede?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section aria-labelledby={id} data-section={id} className="flex flex-col gap-5">
      <SectionHead id={id} title={title}>
        {lede ? <p className="m-0 max-w-2xl text-sm text-ink-soft">{lede}</p> : null}
      </SectionHead>
      {children}
    </section>
  );
}

/** A calm line where a section has nothing to claim: no sheet, no empty chart. */
function Quiet({ children }: { children: ReactNode }) {
  return (
    <p
      data-quiet=""
      className="m-0 max-w-2xl text-base text-ink-soft [font-variant-numeric:lining-nums]"
    >
      {children}
    </p>
  );
}

const rows =
  "m-0 flex list-none flex-col divide-y divide-rule rounded-sheet bg-sheet p-0 px-(--space-panel-pad) py-1";

/**
 * Where a claim sits against what it beats: the 80% range as a `good` bar and the reference (the
 * band median, or the earlier games' mean) as an `ink` tick, on the row's own scale. The words
 * beside it carry the numbers; this only shows the gap at a glance.
 */
function Strip({ low, high, at, label }: { low: number; high: number; at: number; label: string }) {
  if (![low, high, at].every(fin) || high < low) return null;
  const lo = Math.min(low, at);
  const hi = Math.max(high, at);
  const pad = (hi - lo) * 0.15 || 1;
  const x = (v: number) => ((v - (lo - pad)) / (hi - lo + 2 * pad)) * 100;
  return (
    <svg
      aria-hidden="true"
      data-strip=""
      viewBox="0 0 100 16"
      preserveAspectRatio="none"
      className="hidden h-4 w-40 shrink-0 md:block"
    >
      <rect x={0} y={7.5} width={100} height={1} className="fill-rule-strong" />
      <rect
        x={x(low)}
        y={4}
        width={Math.max(1.5, x(high) - x(low))}
        height={8}
        rx={2}
        className="fill-good"
      />
      <rect x={x(at) - 0.75} y={0} width={1.5} height={16} className="fill-ink" />
      <title>{label}</title>
    </svg>
  );
}

function ClaimRow({
  id,
  label,
  figure,
  detail,
  strip,
  href,
  tentative = false,
}: {
  id: string;
  label: string;
  figure: string;
  detail: string;
  strip?: ReactNode;
  href?: string;
  /** A likely but not certain claim: its figure in ink, not in the `good` ink of a sure one. */
  tentative?: boolean;
}) {
  return (
    <li data-claim={id} className="flex flex-col gap-1 py-4 [font-variant-numeric:lining-nums]">
      <div className="flex items-baseline justify-between gap-4">
        {href ? (
          <Link
            href={href}
            className="rounded-field text-base font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          >
            {label}
          </Link>
        ) : (
          <span className="text-base font-medium text-ink">{label}</span>
        )}
        <span className="flex items-center gap-4">
          {strip}
          <span
            className={`font-serif text-xl font-semibold [font-variant-numeric:lining-nums_tabular-nums] ${tentative ? "text-ink" : "text-good-text"}`}
          >
            {figure}
          </span>
        </span>
      </div>
      <p className="m-0 max-w-2xl text-sm text-ink-soft">{detail}</p>
    </li>
  );
}

function OwnClaims({ claims }: { claims: OwnClaim[] }) {
  return (
    <ul data-surface="sheet" className={rows}>
      {claims.map((c) => (
        <ClaimRow
          key={c.metric}
          id={c.metric}
          label={c.label}
          figure={figureOf(c.recent.value, c.unit)}
          detail={ownClaimText(c)}
          strip={
            <Strip
              low={c.earlier.value + c.diff.low}
              high={c.earlier.value + c.diff.high}
              at={c.earlier.value}
              label="Your last games' likely range against your earlier average"
            />
          }
        />
      ))}
    </ul>
  );
}

function ownShortfall(t: OwnTrends): string {
  return `Not enough games yet. This compares your last ${RECENT_GAMES} games with the games before them, so it needs ${plural(t.needGames, "more analysed game", "more analysed games")}${t.measured > 0 ? ` (you have ${plural(t.measured, "game", "games")} with these measures)` : ""}.`;
}

function AboveLevel({ above }: { above: StrengthsModel["above"] }) {
  if (above.mode === "peers") {
    const b = above.benchmark;
    return (
      <Section
        id="above"
        title="Where you're above your level"
        lede={
          <>
            Measures where your whole likely range is better than the players you've faced.{" "}
            {benchmarkLabel(b)}. {BENCHMARK_BIAS_TEXT}
          </>
        }
      >
        {above.claims.length === 0 ? (
          <Quiet>
            None of your {plural(above.compared, "measure", "measures")} compared with them is
            clearly better than theirs yet.
          </Quiet>
        ) : (
          <ul data-surface="sheet" className={rows}>
            {above.claims.map((c) => (
              <ClaimRow
                key={c.metric}
                id={c.metric}
                label={c.label}
                figure={figureOf(c.value, c.unit)}
                detail={peerClaimText(c, b)}
                strip={
                  <Strip
                    low={c.low}
                    high={c.high}
                    at={c.peer}
                    label="Your likely range against the players you've faced"
                  />
                }
              />
            ))}
          </ul>
        )}
      </Section>
    );
  }
  if (above.mode === "band") {
    return (
      <Section
        id="above"
        title="Where you're above your level"
        lede="Measures where your whole likely range is better than the typical player in your rating band."
      >
        {above.claims.length === 0 ? (
          <Quiet>
            None of your {plural(above.compared, "measure", "measures")} with a benchmark is clearly
            above the typical player in your rating band yet.
          </Quiet>
        ) : (
          <ul data-surface="sheet" className={rows}>
            {above.claims.map((c) => (
              <ClaimRow
                key={c.metric}
                id={c.metric}
                label={c.label}
                figure={figureOf(c.value, c.unit)}
                detail={bandClaimText(c)}
                strip={
                  <Strip
                    low={c.low}
                    high={c.high}
                    at={c.median}
                    label="Your likely range against your band's median"
                  />
                }
              />
            ))}
          </ul>
        )}
      </Section>
    );
  }
  const t = above.trends;
  return (
    <Section id="above" title="Where you're above your level" lede={OWN_BASELINE_TEXT}>
      {!t.enough ? (
        <Quiet>{ownShortfall(t)}</Quiet>
      ) : t.claims.length === 0 ? (
        <Quiet>
          Nothing stands out yet: none of your measures is clearly better in your last{" "}
          {RECENT_GAMES} games than in your earlier ones.
        </Quiet>
      ) : (
        <OwnClaims claims={t.claims} />
      )}
    </Section>
  );
}

function Improving({
  improving,
  ownAbove,
  scope,
}: {
  improving: StrengthsModel["improving"];
  ownAbove: boolean;
  scope: CoachScope;
}) {
  const { weaknesses, kpis } = improving;
  const rule = `A change shows here once it is confirmed: at least ${pct0(CONFIRM_P_CHANGE)} likely, and a drop of ${pct0(CONFIRM_REL_CHANGE)} or more.`;
  const kpiClaims = kpis?.claims ?? [];
  return (
    <Section id="improving" title="What's improving" lede={rule}>
      {weaknesses.length === 0 ? (
        <Quiet>No weakness has a confirmed improvement yet.</Quiet>
      ) : (
        <ul data-surface="sheet" className={rows}>
          {weaknesses.map((w) => (
            <ClaimRow
              key={w.seriesKey}
              id={w.seriesKey}
              label={w.label}
              figure={pct1(w.after)}
              detail={weaknessChangeText(w)}
              href={`/app/weaknesses/${encodeURIComponent(w.seriesKey)}?${scopeQuery(scope)}`}
            />
          ))}
        </ul>
      )}
      {ownAbove ? (
        <p className="m-0 max-w-2xl text-sm text-ink-soft">
          Measures that rose against your earlier games are under "Where you're above your level".
        </p>
      ) : kpis && kpiClaims.length > 0 ? (
        <>
          <p className="m-0 max-w-2xl text-sm text-ink-soft">
            Measures better in your last {RECENT_GAMES} games than in your earlier ones:
          </p>
          <OwnClaims claims={kpiClaims} />
        </>
      ) : kpis && !kpis.enough ? (
        <Quiet>{ownShortfall(kpis)}</Quiet>
      ) : null}
    </Section>
  );
}

function Openings({ openings }: { openings: StrengthsModel["openings"] }) {
  return (
    <Section
      id="openings"
      title="Openings that work for you"
      lede={`Lines with at least ${MIN_OPENING_GAMES} games where your whole likely score is above what your rating predicts, then the promising ones: likely above, not yet certain.`}
    >
      {openings.lines.length === 0 ? (
        <Quiet>
          {openings.judged === 0
            ? `Not enough games yet: a line needs at least ${MIN_OPENING_GAMES} analysed games before it is judged.`
            : `None of your ${plural(openings.judged, "line", "lines")} with ${MIN_OPENING_GAMES} or more games scores clearly above what your rating predicts yet.`}
        </Quiet>
      ) : (
        <ul data-surface="sheet" className={rows}>
          {openings.lines.map((o) => (
            <ClaimRow
              key={`${o.color}:${o.kind}:${o.lineKey}`}
              id={o.lineKey}
              label={o.name}
              figure={pct0(o.score)}
              detail={`${o.kind === "chosen" ? "You play it" : "You face it"} as ${o.color === "white" ? "White" : "Black"}. ${openingText(o)}${o.verdict ? ` Verdict: ${VERDICT_TEXT[o.verdict]}.` : ""}`}
              strip={
                <Strip
                  low={o.low}
                  high={o.high}
                  at={o.expected}
                  label="Your likely score against the score your rating predicts"
                />
              }
            />
          ))}
        </ul>
      )}
      {openings.promising.length > 0 ? (
        <div data-promising="" className="flex flex-col gap-3">
          <h3 className="m-0 font-serif text-xl font-semibold text-ink">Promising</h3>
          <p className="m-0 max-w-2xl text-sm text-ink-soft">
            At least {pct0(PROMISING_P)} likely to score above what your rating predicts, but too
            few games to be sure. Keep playing them and the picture firms up.
          </p>
          <ul data-surface="sheet" className={rows}>
            {openings.promising.map((o) => (
              <ClaimRow
                key={`${o.color}:${o.kind}:${o.lineKey}`}
                id={o.lineKey}
                label={o.name}
                figure={pct0(o.score)}
                tentative
                detail={`${o.kind === "chosen" ? "You play it" : "You face it"} as ${o.color === "white" ? "White" : "Black"}. ${promisingText(o)}${o.verdict ? ` Verdict: ${VERDICT_TEXT[o.verdict]}.` : ""}`}
              />
            ))}
          </ul>
        </div>
      ) : null}
    </Section>
  );
}

function waitingText(d: DifficultyStatus): string {
  return d.pendingMaia > 0 && d.positions > 0
    ? ` (${count.format(d.pendingMaia)} of ${count.format(d.positions)} positions waiting)`
    : "";
}

function HardSpots({
  spots,
  difficulty,
}: {
  spots: StrengthsModel["hardSpots"];
  difficulty: DifficultyStatus;
}) {
  const pending = `Difficulty pending: Maia hasn't rated these positions yet${waitingText(difficulty)}.`;
  const fallback = `${pending} Until it does, these are moments where the engine's best move was at least ${CRITICAL_GAP} win% points better than its second choice, your win chance was between ${BALANCED_MIN}% and ${BALANCED_MAX}%, and you found it. Captures of free material are left out; punishing an opponent's mistake with a real move counts.`;
  const lede =
    spots.mode === "maia"
      ? "Positions Maia rates hard for players at your level, where you found the engine's best move."
      : spots.mode === "critical"
        ? fallback
        : difficulty.pendingMaia > 0
          ? pending
          : "Positions where you found the engine's best move when it mattered.";
  return (
    <Section
      id="hard-spots"
      title={spots.mode === "maia" ? HARD_TITLE : CRITICAL_TITLE}
      lede={lede}
    >
      {spots.mode === "none" ? (
        <Quiet>
          No position qualifies yet: none of your analysed games has a critical moment in a balanced
          game where you found the engine's best move.
        </Quiet>
      ) : (
        <ol className="m-0 grid list-none grid-cols-1 gap-x-8 gap-y-10 p-0 md:grid-cols-2 xl:grid-cols-3">
          {spots.spots.map((s) => {
            const note = spots.notes[s.kind] ?? null;
            const move = exemplarMoveText(s.fen, s.san);
            const found = { san: s.san, uci: s.uci };
            return (
              <li
                key={`${s.gameId}:${s.ply}`}
                data-spot={`${s.gameId}:${s.ply}`}
                className="flex min-w-0 flex-col gap-3"
              >
                <GamePosition
                  className="w-full md:w-full"
                  gameId={s.gameId}
                  loadable
                  ply={s.ply}
                  fenBefore={s.fen}
                  played={found}
                  best={found}
                  found
                  orientation={fenMove(s.fen).color}
                  label={move}
                  href={`/app/games/${encodeURIComponent(s.gameId)}?ply=${s.ply}`}
                />
                <div className="flex flex-col gap-2">
                  <p className="m-0 font-serif text-prose text-ink [font-variant-numeric:lining-nums_tabular-nums]">
                    <span className="font-semibold">
                      {move}
                      <span className="tracking-(--tracking-glyph) text-good-text">!</span>
                    </span>{" "}
                    {spots.mode === "maia"
                      ? "You found a hard move."
                      : "You found the only good move."}
                  </p>
                  <p className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">
                    {spotText(s, spots.mode === "maia" ? "maia" : "critical")}
                  </p>
                  {note ? <p className="m-0 font-serif text-base text-ink">{note}</p> : null}
                  <p className="m-0 text-sm text-ink-soft">
                    Played <time dateTime={iso(s.playedAt)}>{dayText(iso(s.playedAt))}</time>
                  </p>
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </Section>
  );
}

function ReportStrengths({ report }: { report: StrengthsModel["report"] }) {
  if (!report || report.strengths.length === 0) return null;
  const at = iso(report.at);
  return (
    <div data-report-strengths="" className="flex max-w-2xl flex-col gap-2">
      <p className="m-0 text-sm text-ink-soft">
        Your report of <time dateTime={at}>{dayText(at)}</time> names{" "}
        {report.strengths.length === 1 ? "this strength" : "these strengths"}, as your coach does:
      </p>
      <ul className="m-0 flex list-none flex-wrap gap-x-6 gap-y-1 p-0">
        {report.strengths.map((s) => (
          <li key={`${s.area}:${s.label}`} className="text-base text-ink">
            <span className="font-medium">{s.label}</span>{" "}
            <span className="text-sm text-ink-soft">({AREA_GLOSSARY[s.area]?.name ?? s.area})</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function StrengthsHeader({ scope }: { scope: CoachScope | null }) {
  return (
    <header className="flex max-w-2xl flex-col gap-2">
      <h1 className="font-serif text-3xl font-semibold text-ink">Strengths</h1>
      <p className="text-lg text-ink-soft [font-variant-numeric:lining-nums]">
        {scope
          ? `${basisText(scope)} What you do well, and what to keep doing.`
          : "What you do well, and what to keep doing."}
      </p>
    </header>
  );
}

export function StrengthsView({
  scope,
  model,
}: {
  scope: CoachScope | null;
  model: StrengthsModel | null;
}) {
  return (
    <div className="flex flex-col gap-12">
      <div className="flex flex-col gap-5">
        <StrengthsHeader scope={scope} />
        {scope ? <CoachScopeSwitch scope={scope} basePath="/app/strengths" /> : null}
        {model ? <ReportStrengths report={model.report} /> : null}
      </div>
      {scope === null || model === null ? (
        <EmptySheet
          action={
            <Button asChild variant="outline">
              <Link href="/app/games">Your games</Link>
            </Button>
          }
        >
          {EMPTY_TEXT}
        </EmptySheet>
      ) : (
        <>
          <AboveLevel above={model.above} />
          <Improving
            improving={model.improving}
            ownAbove={model.above.mode === "own" && model.above.trends.claims.length > 0}
            scope={scope}
          />
          <Openings openings={model.openings} />
          <HardSpots spots={model.hardSpots} difficulty={model.difficulty} />
        </>
      )}
    </div>
  );
}
