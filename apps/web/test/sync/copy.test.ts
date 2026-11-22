import { describe, expect, it } from "vitest";
import {
  agoText,
  analysisLine,
  analysisTotal,
  cooldownText,
  etaText,
  failedText,
  syncLine,
} from "../../src/components/sync/copy";
import type { AccountSync, AnalysisCounts } from "../../src/server/sync";

const NOW = new Date("2026-11-22T12:00:00.000Z");
const ZERO: AnalysisCounts = {
  waiting: 0,
  analysing: 0,
  done: 0,
  failed: 0,
  retrying: 0,
  notSelected: 0,
  skipped: 0,
};
const account = (over: Partial<AccountSync> = {}): AccountSync => ({
  accountId: "a1",
  platform: "chesscom",
  name: "alice",
  verified: true,
  phase: "idle",
  lastCheck: null,
  lastRunFailed: false,
  nextManualAt: null,
  analysis: ZERO,
  eta: null,
  profileUpdating: false,
  ...over,
});
const ago = (ms: number) => new Date(NOW.getTime() - ms).toISOString();
const BAD = /NaN|undefined|Infinity|null/;

describe("agoText", () => {
  it("rounds to minutes, hours, then a UTC day", () => {
    expect(agoText(ago(20_000), NOW)).toBe("just now");
    expect(agoText(ago(3 * 60_000 + 10_000), NOW)).toBe("3 min ago");
    expect(agoText(ago(2 * 3_600_000 + 5 * 60_000), NOW)).toBe("2 h ago");
    expect(agoText("2026-11-12T23:30:00.000Z", NOW)).toBe("on 12 Nov 2026");
  });

  it("a clock ahead of the server reads as just now", () => {
    expect(agoText(new Date(NOW.getTime() + 5000).toISOString(), NOW)).toBe("just now");
  });
});

describe("syncLine", () => {
  it("says what the last check found", () => {
    const lastCheck = { at: ago(3 * 60_000), found: 12 };
    expect(syncLine(account({ lastCheck }), NOW)).toBe("Synced 3 min ago. 12 new games found.");
    expect(syncLine(account({ lastCheck: { ...lastCheck, found: 1 } }), NOW)).toBe(
      "Synced 3 min ago. 1 new game found.",
    );
    expect(syncLine(account({ lastCheck: { ...lastCheck, found: 0 } }), NOW)).toBe(
      "Synced 3 min ago. No new games.",
    );
  });

  it("checking, with the stream's count when there is one", () => {
    expect(syncLine(account({ phase: "checking" }), NOW)).toBe("Checking for new games…");
    expect(syncLine(account({ phase: "checking" }), NOW, 7)).toBe(
      "Checking for new games… 7 new so far.",
    );
  });

  it("paused, failed and never synced", () => {
    expect(syncLine(account({ phase: "paused" }), NOW)).toBe(
      "Sync paused while the site is busy. It carries on by itself.",
    );
    expect(syncLine(account({ lastRunFailed: true }), NOW)).toBe(
      "The last sync didn't finish. Sync now to try again.",
    );
    expect(syncLine(account(), NOW)).toBe("Not synced yet.");
  });
});

describe("cooldownText", () => {
  it("rounds the wait up to whole minutes, and is null once allowed", () => {
    expect(cooldownText(new Date(NOW.getTime() + 61_000).toISOString(), NOW)).toBe(
      "You can sync again in 2 min.",
    );
    expect(cooldownText(new Date(NOW.getTime() + 20_000).toISOString(), NOW)).toBe(
      "You can sync again in 1 min.",
    );
    expect(cooldownText(ago(1000), NOW)).toBeNull();
    expect(cooldownText(null, NOW)).toBeNull();
  });
});

describe("analysisLine", () => {
  it("states done of total with what is left", () => {
    const c = { ...ZERO, done: 86, analysing: 2, waiting: 126 };
    expect(analysisTotal(c)).toBe(214);
    expect(analysisLine(c)).toBe("86 of 214 games analysed. 2 analysing now, 126 waiting.");
  });

  it("all done, nothing yet, and only imported", () => {
    expect(analysisLine({ ...ZERO, done: 40 })).toBe("All 40 games analysed.");
    expect(analysisLine({ ...ZERO, done: 1 })).toBe("All 1 game analysed.");
    expect(analysisLine(ZERO)).toBe("No games to analyse yet.");
    expect(analysisLine({ ...ZERO, notSelected: 5 })).toBe(
      "5 imported games are waiting to be picked for analysis.",
    );
  });

  it("a failed game is not counted as done", () => {
    expect(analysisLine({ ...ZERO, done: 3, failed: 1 })).toBe("3 of 4 games analysed.");
  });
});

describe("etaText", () => {
  it("minutes, under a minute and hours, with the basis", () => {
    expect(etaText({ seconds: 14 * 60 - 5, basis: 31 })).toBe(
      "About 14 min to go at the pace of the last 10 minutes (31 games analysed).",
    );
    expect(etaText({ seconds: 20, basis: 6 })).toBe(
      "Under a minute to go at the pace of the last 10 minutes (6 games analysed).",
    );
    expect(etaText({ seconds: 3 * 3600 + 10, basis: 9 })).toMatch(/^About 4 h to go/);
    expect(etaText(null)).toBeNull();
  });
});

describe("failedText", () => {
  it("failed and retrying, never zero", () => {
    expect(failedText({ ...ZERO, failed: 3 })).toBe("3 games couldn't be analysed.");
    expect(failedText({ ...ZERO, failed: 1, retrying: 2 })).toBe(
      "1 game couldn't be analysed. Retrying 2.",
    );
    expect(failedText({ ...ZERO, retrying: 2 })).toBe("Retrying 2 games.");
    expect(failedText(ZERO)).toBeNull();
  });
});

describe("no bad text", () => {
  it("every line is clean for empty and odd input", () => {
    const lines = [
      syncLine(account({ lastCheck: { at: "garbage", found: 3 } }), NOW),
      analysisLine(ZERO),
      cooldownText("garbage", NOW) ?? "",
      etaText({ seconds: Number.NaN, basis: 5 }) ?? "",
    ];
    for (const l of lines) expect(l).not.toMatch(BAD);
  });
});
