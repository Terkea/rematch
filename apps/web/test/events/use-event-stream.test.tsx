// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@ci/ui/overlays", () => ({ toast: vi.fn() }));

const { useEventStream, POLL_INTERVAL_MS } = await import(
  "../../src/lib/realtime/use-event-stream.js"
);
const { readActivity } = await import("../../src/lib/realtime/activity.js");

type Listener = (e: Event) => void;

/** Just enough of EventSource: named events, open, error, close and readyState. */
class FakeEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  static instances: FakeEventSource[] = [];
  readyState = FakeEventSource.CONNECTING;
  private listeners = new Map<string, Set<Listener>>();
  constructor(
    readonly url: string,
    readonly init?: EventSourceInit,
  ) {
    FakeEventSource.instances.push(this);
  }
  addEventListener(type: string, fn: Listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type)?.add(fn);
  }
  removeEventListener(type: string, fn: Listener) {
    this.listeners.get(type)?.delete(fn);
  }
  close() {
    this.readyState = FakeEventSource.CLOSED;
  }
  private dispatch(e: Event) {
    for (const fn of this.listeners.get(e.type) ?? []) fn(e);
  }
  open() {
    this.readyState = FakeEventSource.OPEN;
    act(() => this.dispatch(new Event("open")));
  }
  fail(closed = false) {
    this.readyState = closed ? FakeEventSource.CLOSED : FakeEventSource.CONNECTING;
    act(() => this.dispatch(new Event("error")));
  }
  emit(type: string, id: number, accountId: string, payload: unknown) {
    const data = JSON.stringify({ accountId, payload });
    act(() => this.dispatch(new MessageEvent(type, { data, lastEventId: String(id) })));
  }
  static get last(): FakeEventSource {
    const es = FakeEventSource.instances.at(-1);
    if (!es) throw new Error("no EventSource opened");
    return es;
  }
}

let qc: QueryClient;
let fetchMock: ReturnType<typeof vi.fn>;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

function pollReply(body: { events: unknown[]; lastId: number; reset: boolean }) {
  fetchMock.mockResolvedValueOnce(Response.json(body));
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.instances = [];
  vi.stubGlobal("EventSource", FakeEventSource);
  fetchMock = vi.fn(async () => Response.json({ events: [], lastId: 0, reset: false }));
  vi.stubGlobal("fetch", fetchMock);
  qc = new QueryClient();
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
  qc.clear();
});

const failure = (reason: string) => ({ kind: "analysis", reason });

describe("useEventStream", () => {
  it("opens one EventSource on /api/events, with the snapshot cursor when given", () => {
    const { result } = renderHook(() => useEventStream({ since: 41 }), { wrapper });
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.last.url).toBe("/api/events?since=41");
    expect(FakeEventSource.last.init?.withCredentials ?? false).toBe(false);
    expect(result.current).toEqual({ mode: "sse", lastEventId: 41 });
  });

  it("opens without a cursor when there is no snapshot, and takes hello's id", () => {
    const { result } = renderHook(() => useEventStream(), { wrapper });
    expect(FakeEventSource.last.url).toBe("/api/events");
    FakeEventSource.last.open();
    FakeEventSource.last.emit("hello", 30, "", {});
    expect(result.current.lastEventId).toBe(30);
  });

  it("applies a duplicate id once", () => {
    const { result } = renderHook(() => useEventStream(), { wrapper });
    const es = FakeEventSource.last;
    es.open();
    es.emit("job.failed", 7, "a1", failure("first"));
    es.emit("job.failed", 7, "a1", failure("again"));
    es.emit("job.failed", 6, "a1", failure("older"));
    expect(readActivity(qc).failures.map((f) => f.reason)).toEqual(["first"]);
    expect(result.current.lastEventId).toBe(7);
  });

  it("switches to polling after three errors without an open, then polls after=<last>", async () => {
    const { result } = renderHook(() => useEventStream({ since: 12 }), { wrapper });
    const es = FakeEventSource.last;
    es.fail();
    es.fail();
    expect(result.current.mode).toBe("sse");
    es.fail();
    expect(result.current.mode).toBe("polling");
    expect(es.readyState).toBe(FakeEventSource.CLOSED);
    expect(fetchMock).not.toHaveBeenCalled();
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[0]).toBe("/api/events?after=12");
  });

  it("keeps the stream after an open, whatever errors follow while the browser reconnects", () => {
    const { result } = renderHook(() => useEventStream(), { wrapper });
    const es = FakeEventSource.last;
    es.open();
    for (let i = 0; i < 5; i++) es.fail();
    expect(result.current.mode).toBe("sse");
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("falls back to polling when the browser gives up on the stream (a 429, say)", () => {
    const { result } = renderHook(() => useEventStream(), { wrapper });
    FakeEventSource.last.open();
    FakeEventSource.last.fail(true);
    expect(result.current.mode).toBe("polling");
  });

  it("starts in polling mode without EventSource", () => {
    vi.stubGlobal("EventSource", undefined);
    const { result } = renderHook(() => useEventStream({ since: 3 }), { wrapper });
    expect(result.current.mode).toBe("polling");
  });

  it("advances lastEventId to the poll's lastId and applies each event once", async () => {
    vi.stubGlobal("EventSource", undefined);
    const { result } = renderHook(() => useEventStream({ since: 3 }), { wrapper });
    pollReply({
      events: [
        { id: 4, accountId: "a1", type: "job.failed", payload: failure("x") },
        { id: 9, accountId: "a1", type: "job.failed", payload: failure("y") },
      ],
      lastId: 9,
      reset: false,
    });
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    expect(result.current.lastEventId).toBe(9);
    pollReply({
      events: [{ id: 9, accountId: "a1", type: "job.failed", payload: failure("y") }],
      lastId: 12,
      reset: false,
    });
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/api/events?after=9");
    expect(result.current.lastEventId).toBe(12);
    expect(readActivity(qc).failures.map((f) => f.reason)).toEqual(["x", "y"]);
  });

  it("polls again at once after a full page of 500 events", async () => {
    vi.stubGlobal("EventSource", undefined);
    renderHook(() => useEventStream({ since: 0 }), { wrapper });
    const events = Array.from({ length: 500 }, (_, i) => ({
      id: i + 1,
      accountId: "a1",
      type: "noise",
      payload: null,
    }));
    // A plain object, not a Response: a 500-event body is read on real timers.
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ events, lastId: 500, reset: false }),
    });
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    for (let i = 0; i < 5; i++) await act(() => vi.advanceTimersByTimeAsync(1));
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/api/events?after=500");
  });

  it("invalidates everything when a poll reports a reset", async () => {
    vi.stubGlobal("EventSource", undefined);
    qc.setQueryData(["accounts"], []);
    renderHook(() => useEventStream({ since: 3 }), { wrapper });
    pollReply({ events: [], lastId: 50, reset: true });
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    expect(qc.getQueryCache().find({ queryKey: ["accounts"] })?.state.isInvalidated).toBe(true);
  });

  it("keeps polling after a failed request", async () => {
    vi.stubGlobal("EventSource", undefined);
    renderHook(() => useEventStream({ since: 3 }), { wrapper });
    fetchMock.mockRejectedValueOnce(new TypeError("offline"));
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS));
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock.mock.calls[1]?.[0]).toBe("/api/events?after=3");
  });

  it("reopens from the last id when reconnectKey changes", () => {
    const { rerender } = renderHook(
      ({ key }: { key: string }) => useEventStream({ since: 1, reconnectKey: key }),
      { wrapper, initialProps: { key: "a1" } },
    );
    const first = FakeEventSource.last;
    first.open();
    first.emit("job.failed", 5, "a1", failure("x"));
    rerender({ key: "a1,a2" });
    expect(first.readyState).toBe(FakeEventSource.CLOSED);
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.last.url).toBe("/api/events?since=5");
  });

  it("does nothing while disabled, and closes on unmount", async () => {
    const { result, rerender, unmount } = renderHook(
      ({ enabled }: { enabled: boolean }) => useEventStream({ enabled }),
      { wrapper, initialProps: { enabled: false } },
    );
    expect(result.current.mode).toBe("off");
    expect(FakeEventSource.instances).toHaveLength(0);
    rerender({ enabled: true });
    const es = FakeEventSource.last;
    unmount();
    expect(es.readyState).toBe(FakeEventSource.CLOSED);
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3));
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
