// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh }),
}));

const { LiveRefreshOn } = await import("../../src/components/live-refresh-on.js");
const { REFRESH_THROTTLE_MS } = await import("../../src/components/live-refresh.js");
const { queryKeys } = await import("../../src/lib/query-keys.js");

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

const invalidate = (key: readonly unknown[]) =>
  act(() => client.invalidateQueries({ queryKey: key, exact: true }));

describe("LiveRefreshOn", () => {
  it("refreshes on its keys only, throttled", async () => {
    const { container } = render(
      <QueryClientProvider client={client}>
        <LiveRefreshOn keys={[queryKeys.report("a1"), queryKeys.weekly("a1")]} />
      </QueryClientProvider>,
    );
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(container).toBeEmptyDOMElement();
    expect(refresh).not.toHaveBeenCalled();

    await invalidate(queryKeys.report("a2"));
    expect(refresh).not.toHaveBeenCalled();

    await invalidate(queryKeys.report("a1"));
    expect(refresh).toHaveBeenCalledTimes(1);
    await invalidate(queryKeys.weekly("a1"));
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(() => vi.advanceTimersByTimeAsync(REFRESH_THROTTLE_MS));
    expect(refresh).toHaveBeenCalledTimes(2);
  });
});
