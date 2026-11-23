"use client";

import { AreaScoreKey, AreaScoreList, areaLabels } from "@ci/ui/area-score";
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

/**
 * "players you've faced on chess.com, rated about 667–967" when the report compared with the
 * peer benchmark (ADRs 0080, 0081), else null: the rating band's words then stand (Q8).
 */
export function peerGroupText(c: ComputedReport): string | null {
  const b = c.benchmark;
  if (b?.source !== "peers" || b.ratingLow === null || b.ratingHigh === null) return null;
  if (!Number.isFinite(b.ratingLow) || !Number.isFinite(b.ratingHigh)) return null;
  return `players you've faced on ${PLATFORM[c.player.platform]}, rated about ${n(b.ratingLow)}–${n(b.ratingHigh)}`;
}

/** "Based on 214 analysed rapid games from 3,912 imported" (and the backfill when it runs). */
export function basisText(c: ComputedReport): string {
  const { analysedGames, importedGames, backfillComplete } = c.basis;
  const games = `${n(analysedGames)} analysed ${c.player.timeClass} ${analysedGames === 1 ? "game" : "games"}`;
  return `Based on ${games} from ${n(importedGames)} imported${backfillComplete ? "" : ", backfill still running"}`;
}

/** Why an area has no score: Mindset by design, otherwise how many more games it needs. */
export function noScoreText(area: string, needMore: number | null): string {
  if (area === "mindset") return "Mindset is coaching advice, not a score";
  if (needMore !== null && Number.isFinite(needMore) && needMore > 0) {
    return `Need ${plural(needMore, "more game", "more games")}`;
  }
  return "No score yet";
}

/**
 * Where you stand: rating and band, the basis line and the area scores on one shared ruler (an
 * area without a score, and Mindset always, says why). All of it is computed, never AI text.
 */
export function Snapshot({ computed, order }: { computed: ComputedReport; order: number }) {
  const { player } = computed;
  const where = `${player.timeClass} on ${PLATFORM[player.platform]}`;
  const rating =
    player.rating !== null && Number.isFinite(player.rating)
      ? `Rated ${n(player.rating)} in ${where}`
      : `Playing ${where}`;
  // Mindset has no score by design (ruling R8), whatever the payload carries.
  const scored = computed.areaScores.filter(
    (a) => a.area !== "mindset" && a.score !== null && Number.isFinite(a.score) && a.sufficient,
  );
  const unscored = computed.areaScores.filter((a) => !scored.includes(a));
  const peers = peerGroupText(computed);
  const band = BAND_TEXT[player.band] ?? "players at your level";
  return (
    <ReportSection id="report-snapshot" title="Where you stand" order={order}>
      <div className="flex max-w-3xl flex-col gap-2">
        <p className="m-0 font-serif text-2xl leading-snug text-ink md:text-3xl">
          {peers ? `${rating}, compared with ${peers}.` : `${rating}, among ${band}.`}
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
                band: peers
                  ? { median: BAND_MEDIAN, name: "players you've faced at" }
                  : { median: BAND_MEDIAN },
                basis: { games: a.n, timeClass: player.timeClass },
              }))}
            />
            <AreaScoreKey
              next={false}
              {...(peers ? { median: "Players you've faced" } : {})}
            />
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
          {peers
            ? `Scores compare you with ${peers}: 50 is level with them and higher is better. A score says how far you are from their rate given how well it is known, not a rank among players.`
            : `Scores are percentiles against ${band}, so 50 is the band median.`}
        </p>
      </div>
    </ReportSection>
  );
}
