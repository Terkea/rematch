// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountSync, AnalysisCounts, SyncStatus } from "../../src/server/sync";
import { stubMatchMedia } from "../ui/helpers";

vi.mock("@ci/ui/overlays", () => ({ toast: vi.fn() }));

const { SyncPanel } = await import("../../src/components/sync/sync-panel.js");

const NOW = "2026-11-22T12:00:00.000Z";
const ZERO: AnalysisCounts = {
  waiting: 0,
  analysing: 0,
  done: 0,
  failed: 0,
  retrying: 0,
  notSelected: 0,
  skipped: 0,
};
const A1 = "11111111-1111-4111-8111-111111111111";
const A2 = "22222222-2222-4222-8222-222222222222";
const account = (over: Partial<AccountSync> = {}): AccountSync => ({
  accountId: A1,
  platform: "chesscom",
  name: "alice",
  verified: true,
  phase: "idle",
  retryAt: null,
  lastCheck: { at: "2026-11-22T11:57:00.000Z", found: 12 },
  lastRunFailed: false,
  nextManualAt: null,
  analysis: { ...ZERO, done: 86, analysing: 2, waiting: 126 },
  eta: null,
  profileUpdating: false,
  ...over,
});
const status = (...accounts: AccountSync[]): SyncStatus => ({ generatedAt: NOW, accounts });

let qc: QueryClient;
let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  stubMatchMedia();
  vi.useFakeTimers({ toFake: ["Date"], now: new Date(NOW) });
  qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  fetchMock = vi.fn();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

function show(s: SyncStatus, variant: "compact" | "full" = "full") {
  return render(
    <QueryClientProvider client={qc}>
      <SyncPanel initial={s} variant={variant} />
    </QueryClientProvider>,
  );
}

const json = (body: unknown, status = 200) =>
  Promise.resolve(new Response(JSON.stringify(body), { status }));

describe("SyncPanel", () => {
  it("shows each account's sync line, progress and Sync now", () => {
    show(status(account()));
    expect(screen.getByText("Synced 3 min ago. 12 new games found.")).toBeInTheDocument();
    expect(
      screen.getByText("86 of 214 games analysed. 2 analysing now, 126 waiting."),
    ).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Sync now chess.com alice" });
    expect(button).toBeEnabled();
    expect(button).not.toHaveAttribute("aria-disabled");
    expect(screen.getByRole("heading", { name: "Sync and analysis" })).toBeInTheDocument();
    expect(screen.queryByText(/to go/)).toBeNull();
  });

  it("Sync now posts the account and shows the check", async () => {
    const user = userEvent.setup({ advanceTimers: () => {} });
    fetchMock.mockImplementation((_url: string, init?: RequestInit) => {
      if (init?.method === "POST") {
        return json({ accountId: A1, outcome: "queued" }, 202);
      }
      return json(status(account({ phase: "checking" })));
    });
    show(status(account()));
    await user.click(screen.getByRole("button", { name: "Sync now chess.com alice" }));
    const post = fetchMock.mock.calls.find((c) => c[1]?.method === "POST");
    expect(post?.[0]).toBe(`/api/accounts/${A1}/sync`);
    expect(await screen.findByText("Checking for new games…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sync now chess.com alice" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("is aria-disabled during the cooldown, says why, and ignores a click", async () => {
    const user = userEvent.setup({ advanceTimers: () => {} });
    show(status(account({ nextManualAt: "2026-11-22T12:01:30.000Z" })));
    const button = screen.getByRole("button", { name: "Sync now chess.com alice" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAccessibleDescription("You can sync again in 2 min.");
    expect(screen.getByText("You can sync again in 2 min.")).toBeInTheDocument();
    await user.click(button);
    expect(fetchMock.mock.calls.filter((c) => c[1]?.method === "POST")).toHaveLength(0);
  });

  it("a held-back import is waiting to retry, with Sync now unavailable", () => {
    show(status(account({ phase: "waiting", retryAt: "2026-11-22T14:05:00.000Z" })));
    expect(
      screen.getByText("Waiting to retry. Next attempt at about 14:05 UTC."),
    ).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "Sync now chess.com alice" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(button).toHaveAccessibleDescription(
      "Waiting to retry. Next attempt at about 14:05 UTC.",
    );
  });

  it("a throttled answer shows the cooldown", async () => {
    const user = userEvent.setup({ advanceTimers: () => {} });
    fetchMock.mockImplementation((_url: string, init?: RequestInit) =>
      init?.method === "POST"
        ? json({ error: "throttled", availableAt: "2026-11-22T12:00:50.000Z" }, 429)
        : json(status(account({ nextManualAt: "2026-11-22T12:00:50.000Z" }))),
    );
    show(status(account()));
    await user.click(screen.getByRole("button", { name: "Sync now chess.com alice" }));
    expect(await screen.findByText("You can sync again in 1 min.")).toBeInTheDocument();
  });

  it("Sync all appears only with two accounts and syncs each ready one", async () => {
    const user = userEvent.setup({ advanceTimers: () => {} });
    const one = show(status(account()));
    expect(screen.queryByRole("button", { name: "Sync all" })).toBeNull();
    one.unmount();
    fetchMock.mockImplementation((_u: string, init?: RequestInit) =>
      init?.method === "POST"
        ? json({ accountId: A1, outcome: "queued" }, 202)
        : json({ accounts: "not a status" }),
    );
    // A later server read (two accounts now) replaces the cached one.
    vi.setSystemTime(new Date("2026-11-22T12:00:05.000Z"));
    show({
      ...status(account(), account({ accountId: A2, platform: "lichess", name: "alice-li" })),
      generatedAt: "2026-11-22T12:00:05.000Z",
    });
    await user.click(screen.getByRole("button", { name: "Sync all" }));
    const posts = fetchMock.mock.calls.filter((c) => c[1]?.method === "POST").map((c) => c[0]);
    expect(posts.sort()).toEqual([`/api/accounts/${A1}/sync`, `/api/accounts/${A2}/sync`]);
  });

  it("shows an ETA only when the worker measured one, with its basis", () => {
    show(status(account({ eta: { seconds: 14 * 60, basis: 31 } })));
    expect(
      screen.getByText(
        "About 14 min to go at the pace of the last 10 minutes (31 games analysed on the server).",
      ),
    ).toBeInTheDocument();
  });

  it("keeps failed games visible with a retry", async () => {
    const user = userEvent.setup({ advanceTimers: () => {} });
    fetchMock.mockImplementation((_u: string, init?: RequestInit) =>
      init?.method === "POST" ? json({ queued: 3 }, 202) : json(status(account())),
    );
    show(status(account({ analysis: { ...ZERO, done: 10, failed: 3 } })));
    expect(screen.getByText("3 games couldn't be analysed.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry 3 games" }));
    const post = fetchMock.mock.calls.find((c) => c[1]?.method === "POST");
    expect(post?.[0]).toBe("/api/sync/retry");
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ accountId: A1 });
  });

  it("a running import shows the stream's count as it arrives", async () => {
    const { queryKeys } = await import("../../src/lib/query-keys.js");
    show(status(account({ phase: "checking" })));
    expect(screen.getByText("Checking for new games…")).toBeInTheDocument();
    act(() => {
      qc.setQueryData(queryKeys.importProgress(A1), { runId: "r", imported: 7 });
    });
    expect(await screen.findByText("Checking for new games… 7 new so far.")).toBeInTheDocument();
  });

  it("says the profile is updating", () => {
    show(status(account({ profileUpdating: true })));
    expect(screen.getByText(/^Profile updating…/)).toBeInTheDocument();
  });

  it("the compact variant is two lines of totals and one button, no tally", () => {
    const { container } = show(
      status(
        account(),
        account({ accountId: A2, name: "bob", analysis: { ...ZERO, done: 4, failed: 1 } }),
      ),
      "compact",
    );
    expect(screen.queryByRole("heading", { name: "Sync and analysis" })).toBeNull();
    expect(screen.queryByRole("list")).toBeNull();
    expect(screen.queryByRole("img")).toBeNull();
    expect(
      screen.getByText(
        "All time controls: 90 of 219 games analysed. 2 analysing now, 126 waiting. 1 couldn't be analysed.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Synced 3 min ago.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Details" })).toHaveAttribute("href", "/app/games");
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Sync all" })).toBeInTheDocument();
    expect(container.querySelectorAll("p")).toHaveLength(2);
  });

  it("the compact variant keeps Verify on a phone, each item its own spaced piece", () => {
    show(status(account({ verified: false })), "compact");
    const verify = screen.getByRole("link", { name: "Verify alice" });
    // Never hidden below sm (P2 Polish: it was missing at 390).
    expect(verify.className).not.toMatch(/max-sm:hidden/);
    expect(verify.closest("[class*='max-sm:hidden']")).toBeNull();
    // The state, Details and Verify are separate items of a wrapping, gapped row, not one run
    // of text ("Details" and "Verify …" ran together).
    const row = verify.parentElement;
    expect(row?.className).toMatch(/(^| )flex-wrap( |$)/);
    expect(row?.className).toMatch(/(^| )gap-x-/);
    expect(screen.getByRole("link", { name: "Details" }).parentElement).toBe(row);
  });

  it("the compact variant with one account syncs that account", () => {
    show(status(account()), "compact");
    expect(screen.getByRole("button", { name: "Sync now chess.com alice" })).toBeInTheDocument();
  });

  it("no tally once every game is analysed", () => {
    show(status(account({ analysis: { ...ZERO, done: 40 } })));
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.getByText("All 40 games analysed.")).toBeInTheDocument();
  });

  it("a 404 or 401 says what happened, not a connection problem", async () => {
    const user = userEvent.setup({ advanceTimers: () => {} });
    const { toast } = await import("@ci/ui/overlays");
    fetchMock.mockImplementation((_u: string, init?: RequestInit) =>
      init?.method === "POST" ? json({ error: "not_found" }, 404) : json(status(account())),
    );
    show(status(account()));
    await user.click(screen.getByRole("button", { name: "Sync now chess.com alice" }));
    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith(
        "This account isn't linked any more. Reload the page to see your accounts.",
      ),
    );
  });

  it("the progress bar states its numbers for assistive tech", () => {
    show(status(account()));
    expect(screen.getAllByRole("img", { name: /86 of 214 analysed/ }).length).toBeGreaterThan(0);
  });

  it("renders no bad text for empty, failed and odd accounts", () => {
    const { container } = show(
      status(
        account({ analysis: ZERO, lastCheck: null }),
        account({ accountId: A2, lastRunFailed: true, analysis: { ...ZERO, notSelected: 4 } }),
      ),
    );
    expect(container.textContent).not.toMatch(/NaN|undefined|Infinity|null/);
    expect(screen.getByText("Not synced yet.")).toBeInTheDocument();
  });

  it("refetches when the stream invalidates the status", async () => {
    fetchMock.mockImplementation(() => json(status(account({ lastCheck: { at: NOW, found: 0 } }))));
    show(status(account()));
    const { syncStatusKey } = await import("../../src/lib/realtime/sync-events.js");
    await qc.invalidateQueries({ queryKey: syncStatusKey() });
    await waitFor(() =>
      expect(screen.getByText("Synced just now. No new games.")).toBeInTheDocument(),
    );
  });
});
