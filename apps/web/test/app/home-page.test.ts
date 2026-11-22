import { beforeEach, describe, expect, it, vi } from "vitest";

// The front door (M5a Task 12): signed in with a linked account goes to the overview, signed in
// with none to Accounts, signed out to sign-in.
let session: { user: { id: string } } | null = null;
let links: string[] = [];
const askedFor: string[] = [];

vi.mock("next/headers", () => ({ headers: async () => new Headers() }));
class Redirect extends Error {
  constructor(readonly to: string) {
    super(`redirect ${to}`);
  }
}
vi.mock("next/navigation", () => ({
  redirect: (to: string) => {
    throw new Redirect(to);
  },
}));
vi.mock("../../src/server/auth/session", () => ({ getSession: async () => session }));
vi.mock("../../src/server/db", () => ({ getDb: () => ({}) }));
vi.mock("../../src/server/events/user-accounts", () => ({
  userAccountIds: async (_db: unknown, userId: string) => {
    askedFor.push(userId);
    return links;
  },
}));

const { default: HomePage } = await import("../../src/app/page");

async function target(): Promise<string> {
  try {
    await HomePage();
  } catch (error) {
    if (error instanceof Redirect) return error.to;
    throw error;
  }
  throw new Error("HomePage did not redirect");
}

beforeEach(() => {
  session = null;
  links = [];
  askedFor.length = 0;
});

describe("home page", () => {
  it("sends a signed-out visitor to /sign-in", async () => {
    expect(await target()).toBe("/sign-in");
  });

  it("sends a signed-in user with no links to /app/accounts", async () => {
    session = { user: { id: "u1" } };
    expect(await target()).toBe("/app/accounts");
    expect(askedFor).toEqual(["u1"]);
  });

  it("sends a signed-in user with a link to the overview", async () => {
    session = { user: { id: "u1" } };
    links = ["a1"];
    expect(await target()).toBe("/app");
  });
});
