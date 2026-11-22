// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import type React from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PENDING_EMAIL_HEADER } from "../../src/server/auth/pending-email.js";
import { stubMatchMedia } from "./helpers.js";

// Review I1: the page shows only the address the proxy handed over, never text from the URL.
let forwarded: Record<string, string> = {};
let blocked = false;
vi.mock("next/headers", () => ({
  headers: async () => new Headers(forwarded),
  cookies: async () => ({ has: (name: string) => blocked && name === "ci_age_block" }),
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

const { default: CheckEmailPage } = await import(
  "../../src/app/(auth)/sign-in/check-email/page.js"
);

beforeEach(() => {
  stubMatchMedia();
  forwarded = {};
  blocked = false;
});

async function renderPage(query: Record<string, string>) {
  // The page takes no searchParams; a caller passing them must change nothing.
  const page = await (CheckEmailPage as (props: unknown) => Promise<React.ReactElement>)({
    searchParams: Promise.resolve(query),
  });
  render(page);
}

describe("/sign-in/check-email", () => {
  it("ignores ?email= entirely", async () => {
    await renderPage({ email: "<b>attacker@evil.example</b> urgent" });
    expect(screen.queryByText(/attacker@evil\.example/)).toBeNull();
    expect(
      screen.getByText("We sent a sign-in link to your email. It expires in 10 minutes."),
    ).toBeInTheDocument();
  });

  it("names the address the proxy handed over", async () => {
    forwarded = { [PENDING_EMAIL_HEADER]: "adult@example.invalid" };
    await renderPage({});
    expect(
      screen.getByText(
        "We sent a sign-in link to adult@example.invalid. It expires in 10 minutes.",
      ),
    ).toBeInTheDocument();
  });

  it("sends a blocked browser back to /sign-in", async () => {
    blocked = true;
    await expect(renderPage({})).rejects.toMatchObject({ to: "/sign-in" });
  });
});
