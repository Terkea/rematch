import type { AccountSync, AnalysisCounts, Eta } from "../../server/sync";

// The words of the sync and analysis panel (Task P2). Every count is stated with its noun, an
// unknown is never shown as 0, and nothing here can print NaN, Infinity, undefined or null.

const MIN = 60_000;
const HOUR = 60 * MIN;

const day = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
  timeZone: "UTC",
});

const plural = (n: number, one: string, many = `${one}s`) => (n === 1 ? one : many);
const games = (n: number) => `${n} ${plural(n, "game")}`;

function time(iso: string | null): number | null {
  if (iso === null) return null;
  const t = Date.parse(iso);
  return Number.isFinite(t) ? t : null;
}

/** "just now", "3 min ago", "2 h ago", then the UTC day ("on 12 Nov 2026"). */
export function agoText(iso: string, now: Date): string {
  const t = time(iso);
  if (t === null) return "a while ago";
  const ms = Math.max(0, now.getTime() - t);
  if (ms < MIN) return "just now";
  if (ms < HOUR) return `${Math.floor(ms / MIN)} min ago`;
  if (ms < 24 * HOUR) return `${Math.floor(ms / HOUR)} h ago`;
  return `on ${day.format(t)}`;
}

/** The account's sync state in one sentence. `found` is the running import's count, if streamed. */
export function syncLine(a: AccountSync, now: Date, found?: number): string {
  if (a.phase === "checking") {
    return found !== undefined && found > 0
      ? `Checking for new games… ${found} new so far.`
      : "Checking for new games…";
  }
  if (a.phase === "paused") return "Sync paused while the site is busy. It carries on by itself.";
  if (a.lastRunFailed) return "The last sync didn't finish. Sync now to try again.";
  if (a.lastCheck === null) return "Not synced yet.";
  const n = a.lastCheck.found;
  const what = n === 0 ? "No new games." : `${n} new ${plural(n, "game")} found.`;
  return `Synced ${agoText(a.lastCheck.at, now)}. ${what}`;
}

/** Until when "Sync now" waits, in whole minutes rounded up; null once it is allowed. */
export function cooldownText(nextManualAt: string | null, now: Date): string | null {
  const t = time(nextManualAt);
  if (t === null || t <= now.getTime()) return null;
  return `You can sync again in ${Math.ceil((t - now.getTime()) / MIN)} min.`;
}

/** Games picked for analysis: done, in progress, waiting, failed and being retried. */
export function analysisTotal(c: AnalysisCounts): number {
  return c.done + c.analysing + c.waiting + c.failed + c.retrying;
}

/** Done of total, then what is left ("2 analysing now, 126 waiting"). */
export function analysisLine(c: AnalysisCounts): string {
  const total = analysisTotal(c);
  if (total === 0) {
    return c.notSelected > 0
      ? `${c.notSelected} imported ${plural(c.notSelected, "game is", "games are")} waiting to be picked for analysis.`
      : "No games to analyse yet.";
  }
  if (c.done === total) return `All ${games(total)} analysed.`;
  const left: string[] = [];
  if (c.analysing > 0) left.push(`${c.analysing} analysing now`);
  if (c.waiting > 0) left.push(`${c.waiting} waiting`);
  const head = `${c.done} of ${games(total)} analysed.`;
  return left.length > 0 ? `${head} ${capital(left.join(", "))}.` : head;
}

const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** The scheduler's ETA in words, with what the pace is measured on; null without one. */
export function etaText(eta: Eta | null): string | null {
  if (eta === null || !Number.isFinite(eta.seconds) || eta.seconds < 0) return null;
  const basis = `at the pace of the last 10 minutes (${games(eta.basis)} analysed).`;
  if (eta.seconds < 60) return `Under a minute to go ${basis}`;
  const minutes = Math.ceil(eta.seconds / 60);
  if (minutes < 120) return `About ${minutes} min to go ${basis}`;
  return `About ${Math.ceil(minutes / 60)} h to go ${basis}`;
}

/** Failed and retrying games; null when there are none. */
export function failedText(c: AnalysisCounts): string | null {
  if (c.failed === 0 && c.retrying === 0) return null;
  if (c.failed === 0) return `Retrying ${games(c.retrying)}.`;
  const head = `${games(c.failed)} couldn't be analysed.`;
  return c.retrying > 0 ? `${head} Retrying ${c.retrying}.` : head;
}

/** Notes under the count: games left out on purpose, so the total adds up. */
export function asideText(c: AnalysisCounts): string | null {
  const parts: string[] = [];
  if (c.notSelected > 0 && analysisTotal(c) > 0) {
    parts.push(`${games(c.notSelected)} more imported, not picked for analysis yet.`);
  }
  if (c.skipped > 0) {
    parts.push(`${games(c.skipped)} can't be analysed (a chess variant, or under two moves).`);
  }
  return parts.length > 0 ? parts.join(" ") : null;
}

export const PLATFORM = { chesscom: "chess.com", lichess: "Lichess" } as const;

/** "chess.com alice", the explorer's account label. */
export const accountLabel = (a: Pick<AccountSync, "platform" | "name">) =>
  `${PLATFORM[a.platform]} ${a.name}`;
