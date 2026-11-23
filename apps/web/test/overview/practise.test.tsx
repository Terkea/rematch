// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PractiseCard } from "../../src/app/(app)/app/_overview/practise-card.js";
import { practiseView } from "../../src/app/(app)/app/_overview/practise-view.js";

// The overview's one call to action (ADR 0092): today's trainer session, done for today, or a
// first run, linking to the trainer on the overview's own scope (and the focus source when set).

vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) => (
    <a href={href} {...rest}>
      {children}
    </a>
  ),
}));

const scope = { accountId: "acc-1", timeClass: "rapid" as const };
const today = "2026-11-23";
const BAD = /NaN|undefined|Infinity|→/;

describe("practiseView", () => {
  it("counts the positions due and links to the trainer on the overview's scope", () => {
    expect(practiseView({ kind: "due", count: 12, source: null }, scope, today)).toEqual({
      kind: "due",
      action: "Practise today: 12 positions due",
      href: "/app/train?account=acc-1&tc=rapid",
      detail: "Positions from your own rapid games where you missed a win or a mate, or blundered.",
    });
    expect(practiseView({ kind: "due", count: 1, source: null }, scope, today)).toMatchObject({
      action: "Practise today: 1 position due",
    });
  });

  it("names the focus source and keeps it in the link", () => {
    const v = practiseView({ kind: "due", count: 4, source: "W4.missed" }, scope, today);
    expect(v.href).toBe("/app/train?account=acc-1&tc=rapid&source=W4.missed");
    expect(v.detail).toMatch(/^From your focus, .+, in your rapid games\.$/);
  });

  it("is done for today, with tomorrow or the next due day", () => {
    expect(practiseView({ kind: "done", next: "2026-11-24", doneToday: 12 }, scope, today)).toEqual(
      {
        kind: "done",
        line: "All done for today, next due tomorrow.",
        detail: "You practised 12 positions today.",
        href: "/app/train?account=acc-1&tc=rapid",
      },
    );
    expect(
      practiseView({ kind: "done", next: "2026-11-30", doneToday: 0 }, scope, today),
    ).toMatchObject({ line: "All done for today, next due on 30 November." });
    expect(practiseView({ kind: "done", next: null, doneToday: 1 }, scope, today)).toMatchObject({
      line: "All done for today.",
      detail:
        "You practised 1 position today. Nothing else is due yet: new positions come from your next analysed games.",
    });
  });

  it("starts training on a first run", () => {
    expect(practiseView({ kind: "first-run" }, scope, today)).toEqual({
      kind: "first-run",
      action: "Start training",
      href: "/app/train?account=acc-1&tc=rapid",
      detail:
        "Practise positions from your own rapid games where you missed a win or a mate, or blundered. Up to 12 a session, about 10 to 15 minutes.",
    });
  });
});

describe("PractiseCard", () => {
  it("is one primary link with the count", () => {
    const { container } = render(
      <PractiseCard view={practiseView({ kind: "due", count: 12, source: null }, scope, today)} />,
    );
    const card = screen.getByRole("region", { name: "Practise" });
    const link = within(card).getByRole("link", { name: "Practise today: 12 positions due" });
    expect(link).toHaveAttribute("href", "/app/train?account=acc-1&tc=rapid");
    expect(within(card).getAllByRole("link")).toHaveLength(1);
    expect(container.innerHTML).not.toMatch(BAD);
  });

  it("says all done, with a quiet way into the trainer", () => {
    render(
      <PractiseCard
        view={practiseView({ kind: "done", next: "2026-11-24", doneToday: 12 }, scope, today)}
      />,
    );
    const card = screen.getByRole("region", { name: "Practise" });
    expect(card).toHaveTextContent("All done for today, next due tomorrow.");
    expect(within(card).getByRole("link", { name: "Open the trainer" })).toBeInTheDocument();
  });

  it("offers Start training on a first run", () => {
    render(<PractiseCard view={practiseView({ kind: "first-run" }, scope, today)} />);
    expect(screen.getByRole("link", { name: "Start training" })).toHaveAttribute(
      "href",
      "/app/train?account=acc-1&tc=rapid",
    );
  });
});
