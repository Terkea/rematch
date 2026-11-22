// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh }),
}));

const { LiveRefresh, REFRESH_THROTTLE_MS } = await import("../src/components/live-refresh.js");
const { queryKeys } = await import("../src/lib/query-keys.js");

let client: QueryClient;

beforeEach(() => {
  vi.useFakeTimers();
  refresh.mockReset();
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
});

afterEach(() => {
  client.clear();
  vi.useRealTimers();
});

async function mount(accountIds: string[]) {
  const view = render(
    <QueryClientProvider client={client}>
      <LiveRefresh accountIds={accountIds} />
    </QueryClientProvider>,
  );
  // Let the observers' first fetch settle: mounting must never refresh.
  await act(() => vi.advanceTimersByTimeAsync(0));
  return view;
}

/** What T23's `applyEvent` does on `game.analysed`. */
async function analysed(accountId: string) {
  await act(() => client.invalidateQueries({ queryKey: queryKeys.games(accountId), exact: true }));
}

describe("LiveRefresh", () => {
  it("throttles to 3000 ms", () => {
    expect(REFRESH_THROTTLE_MS).toBe(3000);
  });

  it("renders nothing and does not refresh on mount", async () => {
    const { container } = await mount(["a1"]);
    expect(container).toBeEmptyDOMElement();
    expect(refresh).not.toHaveBeenCalled();
  });

  it("registers one observer per account, with games(id) keys that never go stale", async () => {
    await mount(["a1", "a2"]);
    for (const id of ["a1", "a2"]) {
      const query = client.getQueryCache().find({ queryKey: queryKeys.games(id), exact: true });
      expect(query?.getObserversCount()).toBe(1);
      expect(query?.isStale()).toBe(false);
    }
  });

  it("refreshes once when a mounted account's games are invalidated", async () => {
    await mount(["a1"]);
    await analysed("a1");
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("gives one call, then one trailing call at 3000 ms, for two invalidations 500 ms apart", async () => {
    await mount(["a1"]);
    await analysed("a1");
    await act(() => vi.advanceTimersByTimeAsync(500));
    await analysed("a1");
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(() => vi.advanceTimersByTimeAsync(2_499));
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(() => vi.advanceTimersByTimeAsync(1));
    expect(refresh).toHaveBeenCalledTimes(2);
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("never refreshes more than once per 3 s under a burst of 50 events", async () => {
    await mount(["a1", "a2"]);
    const calls: number[] = [];
    refresh.mockImplementation(() => calls.push(Date.now()));
    const start = Date.now();
    for (let i = 0; i < 50; i++) {
      await analysed(i % 2 === 0 ? "a1" : "a2");
      await act(() => vi.advanceTimersByTimeAsync(100));
    }
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    // 50 events over 5 s: a leading call, then one per 3 s window, then the trailing one.
    expect(calls.length).toBe(3);
    expect(calls[0]).toBe(start);
    for (let i = 1; i < calls.length; i++) {
      expect((calls[i] ?? 0) - (calls[i - 1] ?? 0)).toBeGreaterThanOrEqual(3000);
    }
  });

  it("ignores accounts it does not watch", async () => {
    await mount(["a1"]);
    // Other queries are cached (another page's), so only the key filter keeps them out.
    client.setQueryData(queryKeys.games("a2"), 1);
    client.setQueryData(queryKeys.accounts(), []);
    client.setQueryData(["games", "a1", "extra"], 1);
    await analysed("a2");
    await act(() => client.invalidateQueries({ queryKey: ["games", "a1", "extra"], exact: true }));
    await act(() => client.invalidateQueries({ queryKey: ["accounts"] }));
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(refresh).not.toHaveBeenCalled();
  });

  it("stops listening once unmounted", async () => {
    const view = await mount(["a1"]);
    view.unmount();
    // Keep the query cached (another page may hold it), so only the subscription decides.
    client.setQueryData(queryKeys.games("a1"), 1);
    await analysed("a1");
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(refresh).not.toHaveBeenCalled();
  });

  it("drops a pending trailing refresh on unmount", async () => {
    const view = await mount(["a1"]);
    await analysed("a1");
    await act(() => vi.advanceTimersByTimeAsync(500));
    await analysed("a1");
    view.unmount();
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
