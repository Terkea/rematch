import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

const toast = vi.fn();
vi.mock("@ci/ui/overlays", () => ({ toast }));

const { applyEvent } = await import("../../src/lib/realtime/reducers.js");
const { EMPTY_ACTIVITY, readActivity } = await import("../../src/lib/realtime/activity.js");
const { queryKeys } = await import("../../src/lib/query-keys.js");

let qc: QueryClient;

beforeEach(() => {
  toast.mockReset();
  qc = new QueryClient();
});

const invalidated = (key: readonly unknown[]) =>
  qc.getQueryCache().find({ queryKey: key, exact: true })?.state.isInvalidated;

describe("applyEvent", () => {
  it("game.analysed patches the cached game and invalidates only that account's games", () => {
    qc.setQueryData(queryKeys.game("g1"), { id: "g1", accuracy: null, analysisStatus: "pending" });
    qc.setQueryData(queryKeys.games("a1"), [1]);
    qc.setQueryData(queryKeys.games("a2"), [2]);
    qc.setQueryData(queryKeys.accounts(), []);
    applyEvent(qc, {
      id: 1,
      accountId: "a1",
      type: "game.analysed",
      payload: { gameId: "g1", accuracy: 81.5 },
    });
    expect(qc.getQueryData(queryKeys.game("g1"))).toEqual({
      id: "g1",
      accuracy: 81.5,
      analysisStatus: "done",
    });
    expect(invalidated(queryKeys.games("a1"))).toBe(true);
    expect(invalidated(queryKeys.games("a2"))).toBe(false);
    expect(invalidated(queryKeys.accounts())).toBe(false);
    expect(invalidated(queryKeys.game("g1"))).toBe(false);
  });

  it("game.analysed never creates a game entry that was not cached", () => {
    applyEvent(qc, {
      id: 1,
      accountId: "a1",
      type: "game.analysed",
      payload: { gameId: "g9", accuracy: 70 },
    });
    expect(qc.getQueryData(queryKeys.game("g9"))).toBeUndefined();
  });

  it("reset invalidates everything", () => {
    qc.setQueryData(queryKeys.games("a1"), [1]);
    qc.setQueryData(queryKeys.accounts(), []);
    qc.setQueryData(["anything", "else"], 1);
    applyEvent(qc, { id: 5, accountId: "", type: "reset", payload: null });
    expect(invalidated(queryKeys.games("a1"))).toBe(true);
    expect(invalidated(queryKeys.accounts())).toBe(true);
    expect(invalidated(["anything", "else"])).toBe(true);
  });

  it("import.progress sets the account's progress and the activity", () => {
    applyEvent(qc, {
      id: 1,
      accountId: "a1",
      type: "import.progress",
      payload: { runId: "r1", imported: 120, total: 400 },
    });
    expect(qc.getQueryData(queryKeys.importProgress("a1"))).toEqual({
      runId: "r1",
      imported: 120,
      total: 400,
    });
    expect(readActivity(qc).imports).toEqual({ a1: { imported: 120, total: 400 } });
  });

  it("a finished import leaves the activity", () => {
    const progress = (payload: object) =>
      applyEvent(qc, { id: 1, accountId: "a1", type: "import.progress", payload });
    progress({ imported: 3 });
    expect(readActivity(qc).imports).toEqual({ a1: { imported: 3 } });
    progress({ imported: 9, done: true });
    expect(readActivity(qc).imports).toEqual({});
    progress({ imported: 4, total: 4 });
    expect(readActivity(qc).imports).toEqual({});
  });

  it("job.failed appends a failure and toasts it, without a modal", () => {
    vi.setSystemTime(new Date("2026-11-22T10:00:00Z"));
    applyEvent(qc, {
      id: 2,
      accountId: "a1",
      type: "job.failed",
      payload: { kind: "analyse-game", reason: "engine crashed" },
    });
    expect(readActivity(qc).failures).toEqual([
      { kind: "analyse-game", reason: "engine crashed", at: Date.parse("2026-11-22T10:00:00Z") },
    ]);
    expect(toast).toHaveBeenCalledWith("A game analysis job failed: engine crashed");
    vi.useRealTimers();
  });

  it("a failed report job refreshes that account's report (its page leaves the building state)", () => {
    qc.setQueryData(queryKeys.report("a1"), 1);
    qc.setQueryData(queryKeys.report("a2"), 1);
    applyEvent(qc, {
      id: 3,
      accountId: "a1",
      type: "job.failed",
      payload: { kind: "generate-report", reason: "ai_unavailable" },
    });
    expect(invalidated(queryKeys.report("a1"))).toBe(true);
    expect(invalidated(queryKeys.report("a2"))).toBe(false);
  });

  it("job.failed names the worker's job kinds in words", () => {
    applyEvent(qc, {
      id: 2,
      accountId: "a1",
      type: "job.failed",
      payload: { kind: "import-account", runId: "r1", reason: "chess.com returned 410" },
    });
    expect(toast).toHaveBeenCalledWith("A games import job failed: chess.com returned 410");
  });

  it("names an unknown kind as sent, with the right article", () => {
    applyEvent(qc, {
      id: 2,
      accountId: "a1",
      type: "job.failed",
      payload: { kind: "export", reason: "disk full" },
    });
    expect(toast).toHaveBeenCalledWith("An export job failed: disk full");
  });

  it("a failed import stops counting as importing", () => {
    applyEvent(qc, {
      id: 1,
      accountId: "a1",
      type: "import.progress",
      payload: { imported: 3 },
    });
    applyEvent(qc, {
      id: 2,
      accountId: "a1",
      type: "job.failed",
      payload: { kind: "import-account", reason: "gone" },
    });
    expect(readActivity(qc).imports).toEqual({});
    expect(readActivity(qc).failures).toHaveLength(1);
  });

  it("ignores unknown events and malformed payloads", () => {
    qc.setQueryData(queryKeys.accounts(), []);
    applyEvent(qc, { id: 1, accountId: "a1", type: "chat.message", payload: { text: "hi" } });
    applyEvent(qc, { id: 2, accountId: "a1", type: "import.progress", payload: "oops" });
    applyEvent(qc, { id: 3, accountId: "a1", type: "game.analysed", payload: null });
    expect(readActivity(qc)).toEqual(EMPTY_ACTIVITY);
    expect(invalidated(queryKeys.accounts())).toBe(false);
    expect(toast).not.toHaveBeenCalled();
  });
});
