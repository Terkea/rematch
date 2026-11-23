// @vitest-environment jsdom
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const refresh = vi.fn();
vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh }) }));

const { FocusCard } = await import("../../src/components/focus/focus-card");
const { MakeFocusButton } = await import("../../src/components/focus/make-focus-button");
const { focusProgress } = await import("../../src/server/focus/goal");

const ACCOUNT = "00000000-0000-4000-8000-000000000001";
const focus = {
  id: "f1",
  accountId: ACCOUNT,
  timeClass: "rapid" as const,
  seriesKey: "W5",
  side: "both" as const,
  label: "Missed mates",
  targetRate: 0.15,
  gamesN: 20,
  startedAt: new Date("2026-11-20T10:00:00Z"),
  baseline: { rate: 0.2, events: 18, chances: 90 },
  peerRate: 0.1,
  endedAt: null,
  outcome: null,
};
const suggestion = {
  accountId: ACCOUNT,
  timeClass: "rapid" as const,
  side: "both" as const,
  seriesKey: "W5",
  label: "Missed mates",
  baseline: { rate: 0.2, events: 18, chances: 90 },
  peer: 0.1,
  target: { rate: 0.15, basis: "relative" as const },
  gamesN: 20,
};

afterEach(() => {
  vi.unstubAllGlobals();
  refresh.mockReset();
});

describe("FocusCard", () => {
  it("shows the running focus: name, goal, games bar, rate vs target with n, and Practise this", () => {
    const progress = focusProgress({ k: 1, n: 6, games: 7, gamesN: 20, target: 0.15 });
    render(<FocusCard card={{ kind: "active", focus, progress }} />);
    const card = screen.getByRole("region", { name: "This week's focus" });
    expect(within(card).getByRole("link", { name: "Missed mates" })).toHaveAttribute(
      "href",
      `/app/weaknesses/W5?account=${ACCOUNT}&tc=rapid`,
    );
    expect(card).toHaveTextContent(
      "Get it down to 15.0% of your chances over your next 20 rapid games, from 20.0% now.",
    );
    const bar = within(card).getByRole("progressbar", { name: "Games played in this focus" });
    expect(bar).toHaveAttribute("aria-valuenow", "7");
    expect(bar).toHaveAttribute("aria-valuemax", "20");
    expect(card).toHaveTextContent("7 of 20 games");
    expect(card).toHaveTextContent("So far 1 of 6 chances, 16.7%");
    expect(card).toHaveTextContent("Too early to tell: 4 more chances");
    expect(within(card).getByRole("link", { name: "Practise this" })).toHaveAttribute(
      "href",
      "/app/train?source=W5",
    );
    expect(card.textContent).not.toMatch(/NaN|undefined|Infinity/);
  });

  // The overview has one call to action, its own "Practise today" (ADR 0092).
  it("leaves out Practise this where the page has its own practise call", () => {
    const progress = focusProgress({ k: 1, n: 6, games: 7, gamesN: 20, target: 0.15 });
    render(<FocusCard card={{ kind: "active", focus, progress }} practise={false} />);
    const card = screen.getByRole("region", { name: "This week's focus" });
    expect(within(card).queryByRole("link", { name: "Practise this" })).toBeNull();
    expect(within(card).getByRole("button", { name: "Clear focus" })).toBeInTheDocument();
  });

  it("offers the suggestion with its target and why", () => {
    render(<FocusCard card={{ kind: "suggest", suggestion }} />);
    const card = screen.getByRole("region", { name: "A focus for this week" });
    expect(card).toHaveTextContent("Missed mates");
    expect(card).toHaveTextContent("That is 18 of 90 chances in the last 6 months.");
    expect(within(card).getByRole("button", { name: "Make this my focus" })).toBeVisible();
    expect(within(card).getByRole("link", { name: "Choose another" })).toHaveAttribute(
      "href",
      `/app/weaknesses?account=${ACCOUNT}&tc=rapid`,
    );
  });

  it("shows a closed focus's outcome and offers the next", () => {
    const progress = {
      ...focusProgress({ k: 1, n: 15, games: 20, gamesN: 20, target: 0.15 }),
      outcome: "met" as const,
    };
    render(
      <FocusCard
        card={{
          kind: "closed",
          focus: { ...focus, outcome: "met", endedAt: new Date("2026-11-22T10:00:00Z") },
          progress,
          next: suggestion,
        }}
      />,
    );
    const card = screen.getByRole("region", { name: "Your last focus" });
    expect(card).toHaveTextContent("Goal met");
    expect(card).toHaveTextContent("1 of 15 chances");
    expect(within(card).getByRole("button", { name: "Make this my next focus" })).toBeVisible();
  });

  it("renders nothing with no focus and nothing to suggest", () => {
    const { container } = render(<FocusCard card={{ kind: "none" }} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe("MakeFocusButton", () => {
  it("posts the weakness and refreshes", async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({}), { status: 201 }));
    vi.stubGlobal("fetch", fetchMock);
    render(
      <MakeFocusButton
        target={{ accountId: ACCOUNT, timeClass: "rapid", seriesKey: "W5", side: "both" }}
        current={false}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Make this my focus" }));
    await waitFor(() => expect(refresh).toHaveBeenCalled());
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/api/focus");
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toEqual({
      accountId: ACCOUNT,
      timeClass: "rapid",
      seriesKey: "W5",
      side: "both",
      gamesN: 20,
    });
  });

  it("says when this weakness is already the focus", () => {
    render(
      <MakeFocusButton
        target={{ accountId: ACCOUNT, timeClass: "rapid", seriesKey: "W5", side: "both" }}
        current
      />,
    );
    expect(screen.queryByRole("button", { name: "Make this my focus" })).toBeNull();
    expect(screen.getByText("This is your focus this week.")).toBeVisible();
  });
});
