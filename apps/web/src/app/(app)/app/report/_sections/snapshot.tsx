"use client";

import { AreaScoreKey, AreaScoreList, areaLabels } from "@ci/ui/area-score";
import { AiDisclosure, type AiDisclosureData } from "@ci/ui/coach";
import type { BandId, ComputedReport } from "../../../../../server/coaching/types";
import { ReportSection } from "./shell";

const count = new Intl.NumberFormat("en-GB");
const n = (v: number) => count.format(Math.max(0, Math.trunc(v)));
const plural = (v: number, one: string, many: string) => `${n(v)} ${v === 1 ? one : many}`;

/** Rust `profile_core::bands` in words. */
export const BAND_TEXT: Record<BandId, string> = {
  B0: "players rated under 900",
  B1: "players rated 900–1299",
  B2: "players rated 1300–1699",
  B3: "players rated 1700–1999",
  B4: "players rated 2000–2299",
  B5: "players rated 2300 and above",
};

const PLATFORM = { lichess: "lichess", chesscom: "chess.com" } as const;

/** A percentile area score's band median is 50 by construction (ruling R8). */
const BAND_MEDIAN = 50;

/** "Based on 214 analysed rapid games from 3,912 imported" (and the backfill when it runs). */
export function basisText(c: ComputedReport): string {
  const { analysedGames, importedGames, backfillComplete } = c.basis;
  const games = `${n(analysedGames)} analysed ${c.player.timeClass} ${analysedGames === 1 ? "game" : "games"}`;
  return `Based on ${games} from ${n(importedGames)} imported${backfillComplete ? "" : ", backfill still running"}`;
}

/** Why an area has no score: Mindset by design, otherwise how many more games it needs. */
function noScoreText(area: string, needMore: number | null): string {
  if (area === "mindset") return "Mindset is coaching advice, not a score";
  if (needMore !== null && Number.isFinite(needMore) && needMore > 0) {
    return `Need ${plural(needMore, "more game", "more games")}`;
  }
  return "No score yet";
}

/**
 * Where you stand: rating and band, the basis line, the seven area scores on one shared ruler (an
 * area without a score says why) and the AI disclosure for the coach's text below.
 */
export function Snapshot({
  computed,
  disclosure,
  order,
}: {
  computed: ComputedReport;
  disclosure: AiDisclosureData | null;
  order: number;
}) {
  const { player } = computed;
  const where = `${player.timeClass} on ${PLATFORM[player.platform]}`;
  const rating =
    player.rating !== null && Number.isFinite(player.rating)
      ? `Rated ${n(player.rating)} in ${where}`
      : `Playing ${where}`;
  const scored = computed.areaScores.filter(
    (a) => a.score !== null && Number.isFinite(a.score) && a.sufficient,
  );
  const unscored = computed.areaScores.filter((a) => !scored.includes(a));
  return (
    <ReportSection id="report-snapshot" title="Where you stand" order={order}>
      <div className="flex max-w-3xl flex-col gap-2">
        <p className="m-0 font-serif text-2xl leading-snug text-ink md:text-3xl">
          {rating}, among {BAND_TEXT[player.band] ?? "players at your level"}.
        </p>
        <p className="m-0 text-sm text-ink-soft">{basisText(computed)}.</p>
      </div>
      <div
        data-surface="sheet"
        className="flex flex-col gap-4 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
      >
        <h3 className="m-0 font-serif text-lg font-semibold text-ink">Your seven areas</h3>
        {scored.length > 0 ? (
          <>
            <AreaScoreList
              items={scored.map((a) => ({
                area: a.area,
                score: a.score as number,
                band: { median: BAND_MEDIAN },
                basis: { games: a.n, timeClass: player.timeClass },
              }))}
            />
            <AreaScoreKey next={false} />
          </>
        ) : null}
        {unscored.length > 0 ? (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {unscored.map((a) => (
              <li key={a.area} className="flex flex-wrap items-baseline gap-x-3 text-sm">
                <span className="font-medium text-ink">{areaLabels[a.area]}</span>
                <span className="text-ink-soft">{noScoreText(a.area, a.needMore)}</span>
              </li>
            ))}
          </ul>
        ) : null}
        <p className="m-0 text-sm text-ink-faint">
          Scores are percentiles against {BAND_TEXT[player.band] ?? "players at your level"}, so 50
          is the band median.
        </p>
      </div>
      {disclosure ? <AiDisclosure disclosure={disclosure} /> : null}
    </ReportSection>
  );
}
