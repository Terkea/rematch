import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { queryKeys } from "../../src/lib/query-keys";
import { applyEvent } from "../../src/lib/realtime/reducers";
import {
  applySyncEvent,
  SYNC_INVALIDATE_MS,
  syncStatusKey,
} from "../../src/lib/realtime/sync-events";

vi.mock("@ci/ui/overlays", () => ({ toast: vi.fn() }));

let qc: QueryClient;
let invalidated: unknown[][];

beforeEach(() => {
  vi.useFakeTimers();
  qc = new QueryClient();
  invalidated = [];
  const original = qc.invalidateQueries.bind(qc);
  qc.invalidateQueries = ((filters?: { queryKey?: unknown[] }) => {
    invalidated.push(filters?.queryKey ?? []);
    return original(filters);
  }) as typeof qc.invalidateQueries;
});

afterEach(() => vi.useRealTimers());

const ev = (type: string, payload: unknown, accountId = "a1") => ({
  id: 1,
  accountId,
  type,
  payload,
  createdAt: "2026-11-22T12:00:00.000Z",
});
const statusCalls = () =>
  invalidated.filter((k) => JSON.stringify(k) === JSON.stringify(syncStatusKey())).length;

describe("applySyncEvent", () => {
  it("a finished import refreshes the status and the account's games", () => {
    applySyncEvent(qc, ev("import.progress", { runId: "r", imported: 4, done: true }) as never);
    expect(statusCalls()).toBe(1);
    expect(invalidated).toContainEqual([...queryKeys.games("a1")]);
  });

  it("progress inside a run does not refetch", () => {
    applySyncEvent(qc, ev("import.progress", { runId: "r", imported: 4 }) as never);
    expect(statusCalls()).toBe(0);
  });

  it("analysis, failures and profile builds refresh the status", () => {
    for (const type of ["game.analysed", "job.failed", "profile.updated", "weakness.changed"]) {
      applySyncEvent(qc, ev(type, {}) as never);
      vi.advanceTimersByTime(SYNC_INVALIDATE_MS);
    }
    expect(statusCalls()).toBe(4);
  });

  it("a burst of analysed games refetches once now and once after the window", () => {
    for (let i = 0; i < 10; i++)
      applySyncEvent(qc, ev("game.analysed", { gameId: `g${i}` }) as never);
    expect(statusCalls()).toBe(1);
    vi.advanceTimersByTime(SYNC_INVALIDATE_MS);
    expect(statusCalls()).toBe(2);
  });

  it("unrelated events do nothing", () => {
    applySyncEvent(qc, ev("chat.chunk", {}) as never);
    expect(invalidated).toEqual([]);
  });

  it("applyEvent passes events through it", () => {
    applyEvent(qc, ev("profile.updated", {}) as never);
    expect(statusCalls()).toBe(1);
  });
});
