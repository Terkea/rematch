// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), replace: vi.fn(), refresh: vi.fn() }),
  usePathname: () => "/",
}));

const { AuthPage } = await import("../src/app/(auth)/auth-page.js");
const NotFound = (await import("../src/app/not-found.js")).default;
const ErrorPage = (await import("../src/app/error.js")).default;
const { OverviewEmpty } = await import("../src/app/(app)/app/_overview/empty-state.js");
const { TrainSummary } = await import("../src/app/(app)/app/train/train-view.js");
const LicensesPage = (await import("../src/app/licenses/page.js")).default;
const { AppFooter } = await import("../src/components/brand/footer.js");

// The logo in the soft places (the owner, 2026-11-23: "the logo in as many places as possible"):
// one mark per view besides the header's, decorative (aria-hidden), never on a board or a chart.

const marks = (c: HTMLElement) => [...c.querySelectorAll("svg[data-brand-mark]")];
const one = (c: HTMLElement) => {
  const all = marks(c);
  expect(all).toHaveLength(1);
  expect(all[0]?.getAttribute("aria-hidden")).toBe("true");
  return all[0];
};

describe("the mark in the soft places", () => {
  it("stands large above the sign-in title", () => {
    const { container } = render(<AuthPage title="Sign in" lede="Your own games." />);
    expect(one(container)?.getAttribute("data-brand-mark")).toBe("full");
  });

  it("opens the 404 page with a line in the coach's voice and a way home", () => {
    const { container } = render(<NotFound />);
    one(container);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("This square is empty");
    expect(screen.getByRole("link", { name: "Back to your games" })).toHaveAttribute("href", "/");
  });

  it("opens the error page with the mark and a retry", () => {
    const reset = vi.fn();
    const { container } = render(<ErrorPage error={new Error("boom")} reset={reset} />);
    one(container);
    screen.getByRole("button", { name: "Try again" }).click();
    expect(reset).toHaveBeenCalled();
    expect(container.textContent).not.toContain("boom");
  });

  it("sits faded in the overview's empty state, beside the margin note", () => {
    const { container } = render(<OverviewEmpty state={{ kind: "no-links" }} />);
    expect(one(container)?.getAttribute("class")).toMatch(/(^| )text-ink-faint( |$)/);
  });

  it("closes the trainer's day with a small mark once it's done", () => {
    const page = { doneToday: 5, rightToday: 4, dueTomorrow: 3 };
    // biome-ignore lint/suspicious/noExplicitAny: a minimal fixture of the fields the summary reads
    const { container } = render(<TrainSummary page={page as any} />);
    one(container);
  });

  it("has no mark on a day with nothing to go over", () => {
    const page = { doneToday: 0, rightToday: 0, dueTomorrow: 0 };
    // biome-ignore lint/suspicious/noExplicitAny: a minimal fixture of the fields the summary reads
    const { container } = render(<TrainSummary page={page as any} />);
    expect(marks(container)).toHaveLength(0);
  });

  it("signs the licences page and the app footer", () => {
    // The licences page carries the header lockup (mid) and the footer's small mark.
    const { container } = render(<LicensesPage />);
    expect(marks(container).map((m) => m.getAttribute("data-brand-mark"))).toEqual(["mid", "mid"]);
    const footer = render(<AppFooter />);
    one(footer.container);
    expect(within(footer.container).getByRole("link", { name: "Licences" })).toHaveAttribute(
      "href",
      "/licenses",
    );
  });
});
