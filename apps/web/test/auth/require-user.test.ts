import { beforeEach, describe, expect, it, vi } from "vitest";

// requireUser is the page-level guard every /app page depends on (review I2): no session goes to
// /sign-in, a session without a birth month goes to /welcome/age, anything else is the user.
const getSession = vi.fn();
vi.mock("../../src/server/auth", () => ({ getAuth: () => ({ api: { getSession } }) }));
vi.mock("next/headers", () => ({
  headers: async () => new Headers({ cookie: "ci.session_token=x" }),
}));
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

const { requireUser } = await import("../../src/server/auth/session");

const user = (birthMonth: string | null) => ({
  user: { id: "u1", email: "adult@example.invalid", name: "", birthMonth },
  session: { id: "s1", expiresAt: new Date(Date.now() + 3600_000) },
});

async function redirectOf(): Promise<string | null> {
  try {
    await requireUser();
    return null;
  } catch (error) {
    if (error instanceof Redirect) return error.to;
    throw error;
  }
}

beforeEach(() => getSession.mockReset());

describe("requireUser", () => {
  it("sends a visitor without a session to /sign-in", async () => {
    getSession.mockResolvedValue(null);
    expect(await redirectOf()).toBe("/sign-in");
  });

  it("sends a signed-in user who hasn't answered the age screen to /welcome/age", async () => {
    getSession.mockResolvedValue(user(null));
    expect(await redirectOf()).toBe("/welcome/age");
  });

  it("returns the user once the age is known, with private defaults", async () => {
    getSession.mockResolvedValue(user("1990-06"));
    await expect(requireUser()).resolves.toMatchObject({
      id: "u1",
      birthMonth: "1990-06",
      marketingEmail: false,
      publicShareDefault: false,
    });
  });
});
