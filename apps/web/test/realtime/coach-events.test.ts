import { QueryClient } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";

const toast = vi.fn();
vi.mock("@ci/ui/overlays", () => ({ toast }));

const { applyCoachEvent } = await import("../../src/lib/realtime/coach-events.js");
const { applyEvent } = await import("../../src/lib/realtime/reducers.js");
const { queryKeys } = await import("../../src/lib/query-keys.js");

let qc: QueryClient;
let id = 0;

beforeEach(() => {
  toast.mockReset();
  qc = new QueryClient();
});

const ev = (type: string, payload: unknown, accountId = "a1") => ({
  id: ++id,
  accountId,
  type,
  payload,
});
const invalidated = (key: readonly unknown[]) =>
  qc.getQueryCache().find({ queryKey: key, exact: true })?.state.isInvalidated;
const seg = (text: string) => [{ kind: "text", text }];
const chunk = (seq: number, text: string, interim = false) =>
  ev("chat.chunk", {
    messageId: "m1",
    seq,
    text,
    segments: seg(text),
    citations: [],
    interim,
  });

describe("applyCoachEvent", () => {
  it("chunks append in seq order and dedupe", () => {
    expect(applyCoachEvent(qc, chunk(2, "b"))).toBe(true);
    applyCoachEvent(qc, chunk(1, "a", true));
    applyCoachEvent(qc, chunk(2, "b"));
    expect(qc.getQueryData(queryKeys.chatStream("m1"))).toEqual({
      chunks: [
        { seq: 1, segments: seg("a"), interim: true },
        { seq: 2, segments: seg("b"), interim: false },
      ],
      done: false,
    });
  });

  it("ready marks the stream done with replace", () => {
    applyCoachEvent(qc, chunk(1, "a", true));
    applyCoachEvent(qc, ev("chat.message.ready", { messageId: "m1", replace: true }));
    expect(qc.getQueryData(queryKeys.chatStream("m1"))).toEqual({
      chunks: [{ seq: 1, segments: seg("a"), interim: true }],
      done: true,
      replace: true,
    });
    // A late chunk after the end changes nothing.
    applyCoachEvent(qc, chunk(2, "late"));
    expect(
      (qc.getQueryData(queryKeys.chatStream("m1")) as { chunks: unknown[] }).chunks,
    ).toHaveLength(1);
  });

  it("failed marks the stream done with its reason", () => {
    applyCoachEvent(qc, ev("chat.message.failed", { messageId: "m2", reason: "weekly_cap" }));
    expect(qc.getQueryData(queryKeys.chatStream("m2"))).toEqual({
      chunks: [],
      done: true,
      reason: "weekly_cap",
    });
  });

  it("report events invalidate only that account's report key", () => {
    for (const k of [
      queryKeys.report("a1"),
      queryKeys.report("a2"),
      queryKeys.weekly("a1"),
      queryKeys.notes("a1"),
    ])
      qc.setQueryData(k, 1);
    applyCoachEvent(
      qc,
      ev("report.section.ready", {
        reportId: "r",
        reportRunId: "rr",
        section: "overview",
        narrative: true,
      }),
    );
    expect(invalidated(queryKeys.report("a1"))).toBe(true);
    expect(invalidated(queryKeys.report("a2"))).toBe(false);
    expect(invalidated(queryKeys.weekly("a1"))).toBe(false);
    expect(invalidated(queryKeys.notes("a1"))).toBe(false);

    qc.setQueryData(queryKeys.report("a2"), 2);
    applyCoachEvent(qc, ev("report.ready", { reportId: "r", status: "ready" }, "a2"));
    expect(invalidated(queryKeys.report("a2"))).toBe(true);

    applyCoachEvent(qc, ev("weekly_review.ready", { id: "w" }));
    expect(invalidated(queryKeys.weekly("a1"))).toBe(true);
    applyCoachEvent(qc, ev("weakness.note.ready", { changeKey: "c", status: "ready" }));
    expect(invalidated(queryKeys.notes("a1"))).toBe(true);
  });

  it("report.degraded invalidates and toasts its reason", () => {
    qc.setQueryData(queryKeys.report("a1"), 1);
    applyCoachEvent(qc, ev("report.degraded", { reportId: "r", reason: "budget", retryAt: null }));
    expect(invalidated(queryKeys.report("a1"))).toBe(true);
    expect(toast).toHaveBeenCalledWith(
      "Your coach's notes are paused: this month's AI spending limit is reached. The computed report is still here.",
    );
  });

  it("malformed payloads are ignored", () => {
    qc.setQueryData(queryKeys.report("a1"), 1);
    for (const bad of [
      ev("chat.chunk", null),
      ev("chat.chunk", { messageId: "m9", seq: -1, segments: [], interim: false }),
      ev("chat.chunk", { messageId: "m9", seq: 1, segments: "x", interim: false }),
      ev("chat.chunk", { messageId: 5, seq: 1, segments: [], interim: false }),
      ev("chat.message.ready", { replace: true }),
      ev("chat.message.failed", { messageId: "m9" }),
      ev("report.degraded", { reportId: "r", reason: 4 }),
      ev("report.ready", "nope"),
      ev("something.else", {}),
    ]) {
      expect(applyCoachEvent(qc, bad)).toBe(false);
    }
    expect(qc.getQueryData(queryKeys.chatStream("m9"))).toBeUndefined();
    expect(invalidated(queryKeys.report("a1"))).toBe(false);
    expect(toast).not.toHaveBeenCalled();
  });

  it("applyEvent routes coach events and still handles game.analysed", () => {
    applyEvent(qc, chunk(1, "a"));
    expect(qc.getQueryData(queryKeys.chatStream("m1"))).toMatchObject({ done: false });
    qc.setQueryData(queryKeys.game("g1"), { id: "g1", accuracy: null, analysisStatus: "pending" });
    qc.setQueryData(queryKeys.games("a1"), [1]);
    applyEvent(qc, ev("game.analysed", { gameId: "g1", accuracy: 80 }));
    expect(qc.getQueryData(queryKeys.game("g1"))).toEqual({
      id: "g1",
      accuracy: 80,
      analysisStatus: "done",
    });
    expect(invalidated(queryKeys.games("a1"))).toBe(true);
  });

  it("query keys", () => {
    expect(queryKeys.report("a")).toEqual(["report", "a"]);
    expect(queryKeys.weekly("a")).toEqual(["weekly", "a"]);
    expect(queryKeys.notes("a")).toEqual(["notes", "a"]);
    expect(queryKeys.thread("t")).toEqual(["thread", "t"]);
    expect(queryKeys.chatStream("m")).toEqual(["chat-stream", "m"]);
  });
});
