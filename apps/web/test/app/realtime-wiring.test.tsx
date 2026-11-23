import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import type { ReactElement, ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

// M4 Task 27 (T23 hand-off): the signed-in layout mounts the one event stream and the header's
// activity indicator, and the stream starts at a cursor read BEFORE any page data, so an event
// stored between the page's read and the stream opening is replayed, never lost. Layout and page
// render in parallel in the App Router, so every signed-in page reads the per-request cursor
// first; the layout reads the same memoised value.

const order: string[] = [];
const USER = { id: "user-1", email: "adult@example.invalid", name: "Ana" };

vi.mock("next/headers", () => ({
  headers: async () => new Headers(),
  cookies: async () => ({ has: () => true, get: () => undefined }),
}));
vi.mock("next/navigation", () => ({
  notFound: () => {
    throw new Error("notFound");
  },
  redirect: (to: string) => {
    throw new Error(`redirect ${to}`);
  },
}));
vi.mock("../../src/server/auth/session", () => ({ requireUser: async () => USER }));
vi.mock("../../src/server/db", () => ({ getDb: () => ({}) }));
vi.mock("../../src/server/events/request-cursor", () => ({
  requestEventCursor: async (userId: string) => {
    order.push(`cursor:${userId}`);
    return 41;
  },
  requestAccountIds: async () => {
    order.push("accounts");
    return ["acc-b", "acc-a"];
  },
}));
vi.mock("../../src/server/overview", () => ({
  getOverview: async () => {
    order.push("data");
    return { timeClasses: [] };
  },
}));
vi.mock("../../src/app/(app)/app/_overview/view", () => ({
  chooseTimeClass: () => null,
  knownTimeClass: () => null,
  parseTimeClass: () => null,
  REVEAL_COOKIE: "reveal",
  toOverviewView: () => ({}),
}));
vi.mock("../../src/server/games", () => ({
  scopedAccountIds: async () => {
    order.push("data");
    return [];
  },
}));
vi.mock("../../src/server/profile", () => ({
  leadFinding: async () => {
    order.push("data");
    return { kind: "building" };
  },
}));
vi.mock("../../src/server/sync", () => ({
  getSyncStatus: async () => {
    order.push("data");
    return { generatedAt: "2026-11-22T12:00:00.000Z", accounts: [] };
  },
}));
vi.mock("../../src/server/games/explorer", () => ({
  loadExplorer: async () => {
    order.push("data");
    return { state: "no-links" };
  },
}));
vi.mock("../../src/server/games/review", () => ({
  getGameReview: async () => {
    order.push("data");
    return null;
  },
}));
vi.mock("../../src/server/accounts/list", () => ({
  listAccounts: async () => {
    order.push("data");
    return [];
  },
}));
vi.mock("../../src/env", () => ({ getEnv: () => ({ CHESSCOM_ENABLED: true }) }));
vi.mock("../../src/server/auth/lichess", () => ({ lichessProvider: () => null }));

const { default: AppLayout } = await import("../../src/app/(app)/layout");
const { RealtimeProvider } = await import("../../src/lib/realtime/realtime-provider");
const { ActivityIndicator } = await import("../../src/components/activity-indicator");
const { Header } = await import("../../src/app/(app)/_shell/header");

type El = ReactElement<{ children?: ReactNode } & Record<string, unknown>>;

/** Every element in a rendered (not mounted) tree, depth first. */
function elements(node: ReactNode): El[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!node || typeof node !== "object" || !("props" in node)) return [];
  const el = node as El;
  const nested = Object.values(el.props).flatMap((value) =>
    value && typeof value === "object" ? elements(value as ReactNode) : [],
  );
  return [el, ...nested];
}

beforeEach(() => {
  order.length = 0;
});

describe("the signed-in layout", () => {
  it("wraps the shell in RealtimeProvider at the request's event cursor", async () => {
    const tree = elements(await AppLayout({ children: <p>page</p> }));
    const provider = tree.find((el) => el.type === RealtimeProvider);
    expect(provider?.props.initialEventId).toBe(41);
    expect(provider?.props.accountIds).toEqual(["acc-b", "acc-a"]);
    // The header and the page are inside it, so both see one stream.
    const inside = elements(provider?.props.children);
    expect(inside.some((el) => el.type === Header)).toBe(true);
    expect(inside.some((el) => el.type === "p")).toBe(true);
  });

  it("reads the cursor before anything else", async () => {
    await AppLayout({ children: null });
    expect(order[0]).toBe("cursor:user-1");
  });

  it("puts the activity indicator in the header's activity slot", async () => {
    const header = elements(await AppLayout({ children: null })).find((el) => el.type === Header);
    const activity = header?.props.activity as El | undefined;
    expect(activity?.type).toBe(ActivityIndicator);
  });
});

describe("every signed-in page reads the cursor before its data", () => {
  it("the overview", async () => {
    const { default: Page } = await import("../../src/app/(app)/app/page");
    await Page({ searchParams: Promise.resolve({}) });
    expect(order[0]).toBe("cursor:user-1");
    expect(order).toContain("data");
  });

  it("the games explorer", async () => {
    const { default: Page } = await import("../../src/app/(app)/app/games/page");
    await Page({ searchParams: Promise.resolve({}) });
    expect(order[0]).toBe("cursor:user-1");
    expect(order).toContain("data");
  });

  it("accounts", async () => {
    const { default: Page } = await import("../../src/app/(app)/app/accounts/page");
    await Page();
    expect(order[0]).toBe("cursor:user-1");
    expect(order).toContain("data");
  });

  it("the game review", async () => {
    const { default: Page } = await import("../../src/app/(app)/app/games/[gameId]/page");
    await expect(
      Page({ params: Promise.resolve({ gameId: "g" }), searchParams: Promise.resolve({}) }),
    ).rejects.toThrow("notFound");
    expect(order[0]).toBe("cursor:user-1");
    expect(order).toContain("data");
  });
});

describe("every page under (app)", () => {
  // review-t27 M7: a new signed-in page must read the cursor too, or the stream can skip events
  // stored while it renders.
  const root = "src/app/(app)";
  const pages = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return pages(path);
      return entry.name === "page.tsx" ? [path] : [];
    });

  it("imports requestEventCursor", () => {
    const found = pages(root);
    expect(found.length).toBeGreaterThanOrEqual(4);
    const missing = found.filter(
      (path) => !/requestEventCursor\(user\.id\)/.test(readFileSync(path, "utf8")),
    );
    expect(missing.map((path) => relative(root, path))).toEqual([]);
  });
});
