// @vitest-environment jsdom
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AccountsView } from "../../src/app/(app)/app/accounts/accounts-view.js";
import type { LinkedAccountView } from "../../src/server/accounts/list.js";
import { renderWithProviders, stubFetch, stubMatchMedia } from "./helpers.js";

const linkId = "0b9d6f5e-2b1a-4c8e-9d0f-1a2b3c4d5e6f";

const pending: LinkedAccountView = {
  linkId,
  platform: "chesscom",
  username: "fakeplayer",
  displayName: "FakePlayer",
  verified: false,
  status: "pending_verification",
  code: "CI-ABC234",
};
const verified: LinkedAccountView = { ...pending, verified: true, status: "ok", code: undefined };

beforeEach(() => stubMatchMedia());
afterEach(() => vi.unstubAllGlobals());

// The empty line names only what is offered: chess.com alone while Lichess is off (ADR 0040).
const EMPTY = "Connect your chess.com account to import your games.";
const EMPTY_BOTH = "Connect a Lichess or chess.com account to import your games.";
const INSTRUCTIONS =
  "Add CI-ABC234 to the Location field on your chess.com profile, then check again. chess.com can take up to 12 hours to show profile changes.";

describe("AccountsView", () => {
  it("shows the heading and the empty state", () => {
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess={false} />);
    expect(screen.getByRole("heading", { name: "Accounts", level: 1 })).toBeInTheDocument();
    expect(screen.getByText(EMPTY)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Connect Lichess" })).toBeNull();
  });

  it("names only chess.com in the empty line while Lichess is off", () => {
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess={false} />);
    expect(screen.getByText(EMPTY)).toBeInTheDocument();
    expect(screen.queryByText(EMPTY_BOTH)).toBeNull();
  });

  it("names both platforms in the empty line when Lichess is on", () => {
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess />);
    expect(screen.getByText(EMPTY_BOTH)).toBeInTheDocument();
    expect(screen.queryByText(EMPTY)).toBeNull();
  });

  it("hides the chess.com form when chess.com is off, and shows Connect Lichess when on", () => {
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom={false} lichess />);
    expect(screen.queryByLabelText("chess.com username")).toBeNull();
    expect(screen.getByRole("button", { name: "Connect Lichess" })).toBeInTheDocument();
  });

  it("connects chess.com, toasts, and shows the pending instructions", async () => {
    const calls = stubFetch([
      {
        method: "POST",
        path: "/api/accounts/chesscom",
        status: 201,
        body: { linkId, username: "fakeplayer", code: "CI-ABC234", verified: false },
      },
      { method: "GET", path: "/api/accounts", body: [pending] },
    ]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess={false} />);
    await userEvent.type(screen.getByLabelText("chess.com username"), "FakePlayer");
    await userEvent.click(screen.getByRole("button", { name: "Connect chess.com" }));
    expect(await screen.findByText("chess.com connected")).toBeInTheDocument();
    expect(await screen.findByText(INSTRUCTIONS)).toBeInTheDocument();
    const post = calls.find((c) => c.method === "POST");
    expect(post?.body).toEqual({ username: "FakePlayer" });
    expect(post?.headers.get("content-type")).toBe("application/json");
    // The first check waits out chess.com's per-username minute (ADR-WEB-5).
    expect(screen.getByRole("button", { name: "Check again" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByText(/You can check again in \d+ s/)).toBeInTheDocument();
  });

  it("explains a username chess.com doesn't know", async () => {
    stubFetch([
      { method: "POST", path: "/api/accounts/chesscom", status: 404, body: { error: "not_found" } },
      { method: "GET", path: "/api/accounts", body: [] },
    ]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess={false} />);
    await userEvent.type(screen.getByLabelText("chess.com username"), "nobody123");
    await userEvent.click(screen.getByRole("button", { name: "Connect chess.com" }));
    expect(
      await screen.findByText("chess.com has no player called nobody123."),
    ).toBeInTheDocument();
  });

  it("copies the code", async () => {
    stubFetch([{ method: "GET", path: "/api/accounts", body: [pending] }]);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    renderWithProviders(<AccountsView initialAccounts={[pending]} chesscom lichess={false} />);
    expect(screen.getByText(INSTRUCTIONS)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copy code" }));
    expect(writeText).toHaveBeenCalledWith("CI-ABC234");
  });

  it("says not there yet for a pending check, and waits until nextCheckAt", async () => {
    stubFetch([
      {
        method: "POST",
        path: `/api/accounts/chesscom/${linkId}/verify`,
        body: { status: "pending", nextCheckAt: new Date(Date.now() + 60_000).toISOString() },
      },
      { method: "GET", path: "/api/accounts", body: [pending] },
    ]);
    renderWithProviders(<AccountsView initialAccounts={[pending]} chesscom lichess={false} />);
    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByText("Not there yet. Try again in a minute.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check again" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
  });

  it("toasts when the account is verified", async () => {
    stubFetch([
      {
        method: "POST",
        path: `/api/accounts/chesscom/${linkId}/verify`,
        body: { status: "verified", nextCheckAt: new Date().toISOString() },
      },
      { method: "GET", path: "/api/accounts", body: [verified] },
    ]);
    renderWithProviders(<AccountsView initialAccounts={[pending]} chesscom lichess={false} />);
    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByText("chess.com account verified")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("Verified")).toBeInTheDocument());
  });

  it("removes an account after confirming in a dialog", async () => {
    const calls = stubFetch([
      { method: "DELETE", path: `/api/accounts/${linkId}`, status: 204 },
      { method: "GET", path: "/api/accounts", body: [] },
    ]);
    renderWithProviders(<AccountsView initialAccounts={[verified]} chesscom lichess={false} />);
    // Each row's Remove names its account (several rows can exist).
    await userEvent.click(screen.getByRole("button", { name: "Remove FakePlayer" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Remove FakePlayer?")).toBeInTheDocument();
    expect(
      within(dialog).getByText("Your imported games stay, but they won't sync to your account."),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove account" }));
    expect(await screen.findByText(EMPTY)).toBeInTheDocument();
    expect(calls.some((c) => c.method === "DELETE")).toBe(true);
  });

  it("with nothing linked, leads with the connect form and the three steps", () => {
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess={false} />);
    expect(screen.queryByRole("heading", { name: "Linked accounts" })).toBeNull();
    const steps = screen.getByRole("list", { name: "How linking works" });
    expect(
      within(steps)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual([
      "1Your chess.com usernameWe check the account exists.",
      "2Put the code in LocationOn your chess.com profile. Remove it afterwards.",
      "3Check againThe account is yours once chess.com shows the code.",
    ]);
  });

  it("uses signed-in copy when Lichess wasn't connected", async () => {
    window.history.replaceState(null, "", "/app/accounts?error=lichess_denied");
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess />);
    expect(
      await screen.findByText("Lichess wasn't connected. You can try again."),
    ).toBeInTheDocument();
    expect(window.location.search).toBe("");
  });

  it.each([
    ["lichess_taken", "This Lichess account is connected to another user."],
    ["lichess_failed", "Connecting Lichess didn't finish. Try again."],
  ])("says why Lichess wasn't connected (%s)", async (code, message) => {
    window.history.replaceState(null, "", `/app/accounts?error=${code}`);
    stubFetch([{ method: "GET", path: "/api/accounts", body: [] }]);
    renderWithProviders(<AccountsView initialAccounts={[]} chesscom lichess />);
    expect(await screen.findByText(message)).toBeInTheDocument();
    expect(window.location.search).toBe("");
  });

  it("offers Reconnect Lichess for an expiring Lichess link", () => {
    const lichess: LinkedAccountView = {
      linkId,
      platform: "lichess",
      username: "fakeuser",
      displayName: "FakeUser",
      verified: true,
      status: "expiring",
    };
    stubFetch([{ method: "GET", path: "/api/accounts", body: [lichess] }]);
    renderWithProviders(<AccountsView initialAccounts={[lichess]} chesscom lichess />);
    expect(screen.getByRole("button", { name: "Reconnect Lichess" })).toBeInTheDocument();
  });
});
