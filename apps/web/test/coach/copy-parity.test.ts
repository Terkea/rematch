import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { type ChatAnchor, fallbackTitle } from "../../src/server/coach/anchor.js";
import { CHAT_COPY, failureCopy } from "../../src/server/coach/copy.js";

const read = (file: string) =>
  readFileSync(new URL(`../../../../backend/crates/ai/src/chat/${file}`, import.meta.url), "utf8");
const limits = read("limits.rs");
const summary = read("summary.rs");

/** A Rust string literal as it appears in source (the copy has no escapes, so JSON's form). */
const literal = (s: string) => JSON.stringify(s);

describe("chat copy mirrors the Rust server copy byte for byte", () => {
  it("fixed texts", () => {
    for (const s of [
      CHAT_COPY.fallback,
      CHAT_COPY.refusal,
      CHAT_COPY.offTopic,
      CHAT_COPY.noData,
      CHAT_COPY.unavailable,
      CHAT_COPY.enginePending,
    ]) {
      expect(limits).toContain(literal(s));
    }
  });

  it("dropped and engine-limit notes", () => {
    expect(limits).toContain(
      literal("I removed {n} statement(s) I couldn't check against your games."),
    );
    expect(CHAT_COPY.droppedNote(2)).toBe(
      "I removed 2 statement(s) I couldn't check against your games.",
    );
    expect(limits).toContain(
      literal("You've used all your engine checks for this {period}. They reset {reset}."),
    );
    expect(limits).toContain('QuotaWindow::Week => ("week", "on Monday")');
    expect(limits).toContain('QuotaWindow::Day => ("day", "tomorrow")');
    expect(limits).toContain('QuotaWindow::Month => ("month", "at the start of next month")');
    expect(CHAT_COPY.engineLimitNote("week")).toBe(
      "You've used all your engine checks for this week. They reset on Monday.",
    );
    expect(CHAT_COPY.engineLimitNote("day")).toBe(
      "You've used all your engine checks for this day. They reset tomorrow.",
    );
  });

  it("cap texts", () => {
    for (const r of [
      "weekly_cap",
      "daily_cap",
      "monthly_cap",
      "conversation_full",
      "too_long",
    ] as const) {
      expect(limits).toContain(literal(CHAT_COPY.capText(r)));
    }
  });

  it("fallback titles", () => {
    const anchors: (ChatAnchor | null)[] = [
      null,
      {
        kind: "report_section",
        reportId: "0b7c6f5e-3d1a-4c2b-9e8f-7a6b5c4d3e2f",
        section: "areas",
      },
      { kind: "weakness", weaknessId: "W1" },
      { kind: "position", gameId: "g", ply: 1 },
      { kind: "move", gameId: "g", ply: 1 },
      { kind: "opening_line", lineKey: "e4" },
      { kind: "game", gameId: "g" },
    ];
    const titles = new Set(anchors.map(fallbackTitle));
    expect(titles.size).toBe(7);
    for (const t of titles) expect(summary).toContain(literal(t));
  });

  it("failure reasons map to fixed copy", () => {
    expect(failureCopy("budget")).toBe(
      "Your coach has reached this month's spending limit. It will be back next month.",
    );
    expect(failureCopy("not_linked")).toBe(
      "This account is no longer linked to you, so the coach can't answer here.",
    );
    expect(failureCopy("lost")).toBe(CHAT_COPY.unavailable);
    expect(failureCopy("unavailable")).toBe(CHAT_COPY.unavailable);
    expect(failureCopy("weekly_cap")).toBe(CHAT_COPY.capText("weekly_cap"));
    expect(failureCopy("conversation_full")).toBe(CHAT_COPY.capText("conversation_full"));
    expect(failureCopy("something_new")).toBe(
      "Your coach couldn't answer that one. Please try again.",
    );
    // The worker's failure reasons (chat/job.rs, jobs/reconcile.rs) are all covered.
    const job = read("job.rs");
    for (const r of ["unavailable", "budget", "not_linked"]) {
      expect(job).toContain(`"${r}"`);
    }
  });
});
