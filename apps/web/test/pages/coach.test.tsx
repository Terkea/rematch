// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { isValidElement, type ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { game } from "../helpers/review-game";

// The "Ask your coach" pages (Task 9): scope and ownership come from the server modules (mocked
// here), bad anchors are dropped, the composer is honest about the allowance, and deleting asks
// first. Synthetic rows only.

const push = vi.fn();
const refresh = vi.fn();
vi.mock("next/navigation", async (orig) => ({
  ...(await orig<object>()),
  useRouter: () => ({ push, replace: vi.fn(), refresh }),
}));
const toast = vi.fn();
vi.mock("@ci/ui/overlays", async (orig) => ({ ...(await orig<object>()), toast }));

const requireUser = vi.fn(async () => ({ id: "u1", email: "x@example.invalid" }));
vi.mock("../../src/server/auth/session", () => ({ requireUser }));
vi.mock("../../src/server/db", () => ({ getDb: () => ({}) }));
const cursorReads: string[] = [];
vi.mock("../../src/server/events/request-cursor", () => ({
  requestEventCursor: async (userId: string) => {
    cursorReads.push(userId);
    return 0;
  },
}));

const resolveCoachScope = vi.fn();
vi.mock("../../src/server/profile/scope", async (orig) => ({
  ...(await orig<object>()),
  resolveCoachScope,
}));
const chatAllowance = vi.fn();
vi.mock("../../src/server/coach/allowance", async (orig) => ({
  ...(await orig<object>()),
  chatAllowance,
}));
const getThread = vi.fn();
const listConversations = vi.fn();
vi.mock("../../src/server/coach/threads", async (orig) => ({
  ...(await orig<object>()),
  getThread,
  listConversations,
}));
const getGameReview = vi.fn();
const reviewGameIds = vi.fn();
vi.mock("../../src/server/profile/game-ids", () => ({ reviewGameIds }));
vi.mock("../../src/server/games/review", async (orig) => ({
  ...(await orig<object>()),
  getGameReview,
}));

const { encodeAnchor } = await import("../../src/server/coach/anchor.js");
const ListPage = (await import("../../src/app/(app)/app/coach/page.js")).default;
const NewPage = (await import("../../src/app/(app)/app/coach/new/page.js")).default;
const ThreadPage = (await import("../../src/app/(app)/app/coach/[threadId]/page.js")).default;
const { ThreadView } = await import("../../src/app/(app)/app/coach/[threadId]/thread-view.js");
const { AnchorCard, loadAnchorView } = await import(
  "../../src/app/(app)/app/coach/[threadId]/anchor-card.js"
);
const { AI_DISCLOSURE_NOTE } = await import("../../src/app/(app)/app/coach/copy.js");
const { applyCoachEvent } = await import("../../src/lib/realtime/coach-events.js");
const { allowanceText } = await import("../../src/app/(app)/app/coach/allowance.js");

const ACCOUNT = "00000000-0000-4000-8000-0000000000a1";
const THREAD = "00000000-0000-4000-8000-0000000000e1";
const GAME = "00000000-0000-4000-8000-000000000001";
/** Anchors and segments name a game by its platform id; the review takes `games.id`. */
const PLATFORM_GAME = "lichAbc1";

const scope = {
  accountId: ACCOUNT,
  timeClass: "rapid" as const,
  platform: "lichess" as const,
  username: "knightly",
  options: [
    {
      accountId: ACCOUNT,
      platform: "lichess" as const,
      username: "knightly",
      timeClass: "rapid" as const,
      analysedGames: 214,
    },
  ],
};

const RESET = new Date("2026-11-23T00:00:00.000Z");
const allowance = (used: number, limit = 3) => ({
  quotas: [{ window: "week", limit, used, resetsAt: RESET }],
  exhausted: used >= limit ? { reason: "weekly_cap", resetsAt: RESET } : null,
});
const view = (used: number, limit = 3) => ({
  left: Math.max(0, limit - used),
  limit,
  window: "week" as const,
  resetsAt: RESET.toISOString(),
  exhausted: used >= limit ? ("weekly_cap" as const) : null,
});

const text = (t: string) => [{ kind: "text" as const, text: t }];
const DISCLOSURE = {
  label: "Written by AI",
  note: "Test note.",
  model: "test-model",
  promptVersion: "chat@1",
  generatedAt: "2026-11-20T10:00:00.000Z",
};

function msg(over: Record<string, unknown>) {
  return {
    id: "00000000-0000-4000-8000-000000000102",
    seq: 2,
    role: "assistant",
    status: "done",
    anchor: null,
    segments: text("Your rook left the seventh rank too early."),
    failureReason: null,
    aiDisclosure: DISCLOSURE,
    createdAt: new Date("2026-11-20T10:00:00.000Z"),
    streamed: [],
    engineChecks: [{ status: "done", moves: ["Nd2", "e4", "Nxe4"], startPly: 55 }],
    ...over,
  };
}

const thread = {
  id: THREAD,
  accountId: ACCOUNT,
  timeClass: "rapid" as const,
  title: "Rook endgames",
  anchor: null,
  messages: [
    msg({
      id: "00000000-0000-4000-8000-000000000101",
      seq: 1,
      role: "user",
      segments: text("Why did I lose the endgame?"),
      aiDisclosure: null,
      engineChecks: [],
    }),
    msg({}),
  ],
  // biome-ignore lint/suspicious/noExplicitAny: the server's Thread shape
} as any;

const fetchMock = vi.fn();

beforeEach(() => {
  for (const m of [push, refresh, toast, resolveCoachScope, chatAllowance, getThread])
    m.mockReset();
  listConversations.mockReset();
  getGameReview.mockReset();
  reviewGameIds.mockReset();
  reviewGameIds.mockImplementation(async (_db, _u, _s, ids: string[]) =>
    ids.includes(PLATFORM_GAME) ? new Map([[PLATFORM_GAME, GAME]]) : new Map(),
  );
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
  resolveCoachScope.mockResolvedValue(scope);
  chatAllowance.mockResolvedValue(allowance(1));
  listConversations.mockResolvedValue([]);
  getThread.mockResolvedValue(thread);
  getGameReview.mockResolvedValue(game(["e4", "e5", "Nf3"]));
});
afterEach(() => vi.unstubAllGlobals());

/** The first element in the tree whose props hold `key`. */
function findProps(node: unknown, key: string): Record<string, unknown> | null {
  if (Array.isArray(node)) {
    for (const n of node) {
      const hit = findProps(n, key);
      if (hit) return hit;
    }
    return null;
  }
  if (!isValidElement(node)) return null;
  const props = node.props as Record<string, unknown>;
  if (key in props) return props;
  return findProps(props.children, key);
}

/** Client views need the app's QueryClient. */
/** Client views need the app's QueryClient (pass one to drive the event reducer). */
function render(ui: ReactElement, qc = new QueryClient()) {
  return rtlRender(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const BAD_TEXT = /NaN|undefined|Infinity|null|\{\{/;

describe("coach pages", () => {
  it("invalid anchor is dropped", async () => {
    const tree = await NewPage({
      searchParams: Promise.resolve({ account: ACCOUNT, tc: "rapid", anchor: "not*an*anchor" }),
    });
    const props = findProps(tree, "anchor");
    expect(props?.anchor).toBeNull();
    expect(getGameReview).not.toHaveBeenCalled();
  });

  it("a move anchor on a game the user cannot see is dropped too", async () => {
    // Not one of the scope account's games: nothing to map, nothing read.
    const anchor = encodeAnchor({ kind: "move", gameId: "someoneElse", ply: 2 });
    const tree = await NewPage({ searchParams: Promise.resolve({ anchor }) });
    expect(findProps(tree, "anchor")?.anchor).toBeNull();
    expect(reviewGameIds).toHaveBeenCalledWith({}, "u1", { accountId: ACCOUNT }, ["someoneElse"]);
    expect(getGameReview).not.toHaveBeenCalled();
    // Mapped, but the review read refuses it.
    getGameReview.mockResolvedValue(null);
    const mapped = encodeAnchor({ kind: "move", gameId: PLATFORM_GAME, ply: 2 });
    const again = await NewPage({ searchParams: Promise.resolve({ anchor: mapped }) });
    expect(findProps(again, "anchor")?.anchor).toBeNull();
    expect(getGameReview).toHaveBeenCalledWith({}, "u1", GAME);
  });

  it("anchor card shows the position for a move anchor", async () => {
    const anchor = { kind: "move" as const, gameId: PLATFORM_GAME, ply: 2 };
    const anchorView = await loadAnchorView({} as never, "u1", ACCOUNT, anchor);
    expect(anchorView).not.toBeNull();
    render(<AnchorCard view={anchorView as NonNullable<typeof anchorView>} />);
    expect(screen.getByRole("region", { name: "About a move" })).toBeInTheDocument();
    expect(screen.getByLabelText("Position after 1… e5")).toBeInTheDocument();
    expect(screen.getByText(/7 Nov 2026, loss v Blacky \(1510\)/)).toBeInTheDocument();
    // The new page hands the decoded anchor to the composer.
    const tree = await NewPage({
      searchParams: Promise.resolve({ anchor: encodeAnchor(anchor) }),
    });
    expect(findProps(tree, "anchor")?.anchor).toEqual(anchor);
  });

  it("a ply past the game's end is dropped", async () => {
    expect(
      await loadAnchorView({} as never, "u1", ACCOUNT, {
        kind: "move",
        gameId: PLATFORM_GAME,
        ply: 9,
      }),
    ).toBe(null);
  });

  it("exhausted allowance disables the composer with the reset copy", () => {
    render(<ThreadView thread={thread} allowance={view(3)} listHref="/app/coach" />);
    const box = screen.getByRole("textbox", { name: "Your question" });
    expect(box).toBeDisabled();
    expect(
      screen.getByText("You've used this week's coach messages. They reset on Monday."),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/0 of 3 coach messages left this week, resets Monday/),
    ).toBeInTheDocument();
    expect(screen.getByText("23 Nov 2026").closest("time")).toHaveAttribute(
      "dateTime",
      "2026-11-23",
    );
  });

  it("a thread shows its answer with the disclosure and the engine check", () => {
    render(<ThreadView thread={thread} allowance={view(1)} listHref="/app/coach" />);
    expect(screen.getByText("Why did I lose the endgame?")).toBeInTheDocument();
    expect(screen.getByText("Your rook left the seventh rank too early.")).toBeInTheDocument();
    expect(screen.getAllByText("Written by AI").length).toBeGreaterThan(0);
    expect(screen.getByText(/28\. Nd2 e4 29\. Nxe4/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Your question" })).toBeEnabled();
  });

  it("a move in an answer links to the review by games.id", () => {
    const linked = {
      ...thread,
      messages: [
        msg({
          segments: [
            { kind: "text", text: "After " },
            { kind: "move", gameId: PLATFORM_GAME, ply: 2, san: "e5" },
            { kind: "text", text: " and " },
            { kind: "move", gameId: "notMapped", ply: 3, san: "Nf3" },
          ],
        }),
      ],
    };
    render(
      <ThreadView
        thread={linked}
        allowance={view(1)}
        listHref="/app/coach"
        gameIds={{ [PLATFORM_GAME]: GAME }}
      />,
    );
    expect(screen.getByRole("link", { name: "e5" })).toHaveAttribute(
      "href",
      `/app/games/${GAME}?ply=2`,
    );
    expect(screen.queryByRole("link", { name: "Nf3" })).toBeNull();
  });

  it("the thread page maps the answers' games for the thread's account", async () => {
    getThread.mockResolvedValue({
      ...thread,
      messages: [msg({ segments: [{ kind: "move", gameId: PLATFORM_GAME, ply: 2, san: "e5" }] })],
    });
    const tree = await ThreadPage({
      params: Promise.resolve({ threadId: THREAD }),
      searchParams: Promise.resolve({}),
    });
    expect(reviewGameIds).toHaveBeenCalledWith({}, "u1", { accountId: ACCOUNT }, [PLATFORM_GAME]);
    expect(findProps(tree, "gameIds")?.gameIds).toEqual({ [PLATFORM_GAME]: GAME });
  });

  it("every coach page reads the event cursor before its data (T27)", async () => {
    cursorReads.length = 0;
    await ListPage({ searchParams: Promise.resolve({}) });
    await NewPage({ searchParams: Promise.resolve({}) });
    await ThreadPage({
      params: Promise.resolve({ threadId: THREAD }),
      searchParams: Promise.resolve({}),
    });
    expect(cursorReads).toEqual(["u1", "u1", "u1"]);
  });

  it("foreign thread is not found", async () => {
    getThread.mockResolvedValue(null);
    await expect(
      ThreadPage({
        params: Promise.resolve({ threadId: THREAD }),
        searchParams: Promise.resolve({}),
      }),
    ).rejects.toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
    expect(getThread).toHaveBeenCalledWith({}, "u1", THREAD);
  });

  it("delete asks for confirmation", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    const user = userEvent.setup();
    render(<ThreadView thread={thread} allowance={view(1)} listHref="/app/coach?account=a" />);
    await user.click(screen.getByRole("button", { name: "Delete conversation" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Delete this conversation?")).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Delete conversation" }));
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/coach/threads/${THREAD}`,
        expect.objectContaining({ method: "DELETE" }),
      ),
    );
    await waitFor(() => expect(push).toHaveBeenCalledWith("/app/coach?account=a"));
    expect(toast).toHaveBeenCalledWith("Conversation deleted");
  });

  it("cancelling the delete deletes nothing", async () => {
    const user = userEvent.setup();
    render(<ThreadView thread={thread} allowance={view(1)} listHref="/app/coach" />);
    await user.click(screen.getByRole("button", { name: "Delete conversation" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("the list shows the threads, the allowance and the disclosure note", async () => {
    listConversations.mockResolvedValue([
      {
        id: THREAD,
        title: "Rook endgames",
        anchor: { kind: "move", gameId: PLATFORM_GAME, ply: 2 },
        lastMessageAt: new Date("2026-11-12T18:00:00.000Z"),
      },
    ]);
    const tree = await ListPage({ searchParams: Promise.resolve({}) });
    const { container } = render(tree as ReactElement);
    const link = screen.getByRole("link", { name: /Rook endgames/ });
    expect(link).toHaveAttribute("href", `/app/coach/${THREAD}`);
    expect(within(link).getByText("About a move")).toBeInTheDocument();
    expect(within(link).getByText("12 Nov 2026").closest("time")).toHaveAttribute(
      "dateTime",
      "2026-11-12",
    );
    expect(screen.getByText(/2 of 3 coach messages left this week, resets Monday/)).toBeVisible();
    expect(screen.getByText(AI_DISCLOSURE_NOTE)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "New conversation" })).toHaveAttribute(
      "href",
      `/app/coach/new?account=${ACCOUNT}&tc=rapid`,
    );
    expect(listConversations).toHaveBeenCalledWith({}, "u1", ACCOUNT);
    expect(container.textContent).not.toMatch(BAD_TEXT);
  });

  it("an empty list invites the first question", async () => {
    const tree = await ListPage({ searchParams: Promise.resolve({}) });
    render(tree as ReactElement);
    expect(
      screen.getByText("Ask about any of your games, moves, openings or weaknesses."),
    ).toBeInTheDocument();
  });

  it("a user with no analysed games is told why there is no coach yet", async () => {
    resolveCoachScope.mockResolvedValue(null);
    const tree = await ListPage({ searchParams: Promise.resolve({}) });
    const { container } = render(tree as ReactElement);
    expect(container.textContent).toMatch(/analysed games/);
    expect(chatAllowance).not.toHaveBeenCalled();
  });

  it("renders no NaN, undefined, Infinity, null or template tokens", async () => {
    const odd = {
      ...thread,
      messages: [
        ...thread.messages,
        msg({ id: "m3", seq: 3, role: "user", segments: null, aiDisclosure: null }),
        msg({ id: "m4", seq: 4, status: "failed", failureReason: "budget", segments: null }),
        msg({
          id: "m5",
          seq: 5,
          status: "done",
          aiDisclosure: null,
          segments: [{ kind: "num", path: "x", value: Number.NaN, display: "" }],
          engineChecks: [{ status: "pending", moves: ["e4"], startPly: null }],
        }),
      ],
    };
    const { container } = render(
      <ThreadView thread={odd} allowance={view(1)} listHref="/app/coach" />,
    );
    expect(container.textContent).not.toMatch(BAD_TEXT);
    expect(
      screen.getByText(
        "Your coach has reached this month's spending limit. It will be back next month.",
      ),
    ).toBeInTheDocument();
  });

  it("the disclosure note is Rust's, byte for byte", () => {
    // A path, not a URL: jsdom replaces the URL global.
    const here = dirname(fileURLToPath(import.meta.url));
    const rust = readFileSync(
      resolve(here, "../../../../backend/crates/ai/src/output/disclosure.rs"),
      "utf8",
    );
    const m = /pub const AI_DISCLOSURE_NOTE: &str = "([^"]*)";/.exec(rust);
    expect(m?.[1]).toBe(AI_DISCLOSURE_NOTE);
  });
});

describe("coach pages, review fixes", () => {
  const Q2 = "00000000-0000-4000-8000-000000000103";
  const A2 = "00000000-0000-4000-8000-000000000104";

  it("I4: focus stays with the conversation through a send and returns to the box", async () => {
    const qc = new QueryClient();
    const answered = {
      ...thread,
      messages: [
        ...thread.messages,
        msg({
          id: Q2,
          seq: 3,
          role: "user",
          segments: text("And the opening?"),
          aiDisclosure: null,
          engineChecks: [],
        }),
        msg({ id: A2, seq: 4, segments: text("Play e4."), engineChecks: [] }),
      ],
    };
    fetchMock.mockImplementation((_url: string, init?: { method?: string }) =>
      init?.method === "POST"
        ? Promise.resolve(
            new Response(JSON.stringify({ userMessageId: Q2, assistantMessageId: A2 }), {
              status: 202,
            }),
          )
        : Promise.resolve(new Response(JSON.stringify(answered), { status: 200 })),
    );
    const user = userEvent.setup();
    render(<ThreadView thread={thread} allowance={view(1)} listHref="/app/coach" />, qc);
    const box = screen.getByRole("textbox", { name: "Your question" });
    await user.click(box);
    await user.keyboard("And the opening?{Enter}");
    // While the coach writes, focus sits on the polite status line, not on <body>.
    const status = await screen.findByText(/Your coach is writing/);
    await waitFor(() => expect(status.closest("[role=status]")).toHaveFocus());
    act(() => {
      applyCoachEvent(qc, {
        id: 1,
        accountId: ACCOUNT,
        type: "chat.message.ready",
        payload: { messageId: A2, attempt: 1, replace: false },
      });
    });
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Your question" })).toHaveFocus(),
    );
  });

  it("V1: a pending answer is one calm line", () => {
    const pending = {
      ...thread,
      messages: [
        thread.messages[0],
        msg({ status: "pending", segments: null, aiDisclosure: null, engineChecks: [] }),
      ],
    };
    render(<ThreadView thread={pending} allowance={view(1)} listHref="/app/coach" />);
    expect(screen.getByText("Reading your games")).toBeInTheDocument();
    expect(screen.queryByText("Your coach is reading your games")).toBeNull();
    expect(screen.queryByText("Writing")).toBeNull();
  });

  it("m2: only the latest failed answer offers Ask again", () => {
    const failed = {
      ...thread,
      messages: [
        thread.messages[0],
        msg({
          status: "failed",
          failureReason: "unavailable",
          segments: null,
          aiDisclosure: null,
          engineChecks: [],
        }),
        msg({
          id: Q2,
          seq: 3,
          role: "user",
          segments: text("Again?"),
          aiDisclosure: null,
          engineChecks: [],
        }),
        msg({
          id: A2,
          seq: 4,
          status: "failed",
          failureReason: "unavailable",
          segments: null,
          aiDisclosure: null,
          engineChecks: [],
        }),
      ],
    };
    render(<ThreadView thread={failed} allowance={view(1)} listHref="/app/coach" />);
    expect(screen.getAllByRole("button", { name: "Ask again" })).toHaveLength(1);
  });

  it("m3: a month window resets on its date, without brackets", () => {
    const t = allowanceText({
      left: 280,
      limit: 300,
      window: "month",
      resetsAt: "2026-12-01T00:00:00.000Z",
      exhausted: null,
    });
    expect(t.text).toBe("280 of 300 coach messages left this month, resets on");
    expect(t.bracketed).toBe(false);
  });
});
