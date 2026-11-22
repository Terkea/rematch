// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const toast = vi.fn();
vi.mock("@ci/ui/overlays", () => ({ toast }));

const { RealtimeProvider } = await import("../../src/lib/realtime/realtime-provider.js");
const { POLL_INTERVAL_MS } = await import("../../src/lib/realtime/use-event-stream.js");

beforeEach(() => {
  vi.useFakeTimers();
  toast.mockReset();
  vi.stubGlobal("EventSource", undefined);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json({ error: "unauthorized" }, { status: 401 })),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("RealtimeProvider", () => {
  it("tells a signed-out user once, with a way back to sign in", async () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <RealtimeProvider accountIds={["a1"]} initialEventId={0}>
          <p>page</p>
        </RealtimeProvider>
      </QueryClientProvider>,
    );
    await act(() => vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3));
    expect(toast).toHaveBeenCalledTimes(1);
    const [message, options] = toast.mock.calls[0] ?? [];
    expect(message).toBe("You're signed out, so updates have stopped.");
    expect(options?.action?.label).toBe("Sign in");
  });
});
