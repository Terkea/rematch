// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { EstimateKpi } from "../../src/components/estimate-kpi.js";
import { gameCaption } from "../../src/components/game-caption.js";
import type { GameRow } from "../../src/server/games/types.js";

// Synthetic values only (B15).

const leaves = (root: HTMLElement) =>
  [...root.querySelectorAll("*")].filter((el) => el.children.length === 0);

describe("EstimateKpi", () => {
  it("asks for more games below the minimum, with its basis and no number", () => {
    const { container } = render(
      <EstimateKpi
        label="Accuracy"
        estimate={{ kind: "insufficient", n: 3, needed: 7 }}
        format="percent"
        basisClass="rapid"
      />,
    );
    expect(screen.getByText("Need 7 more games")).toBeInTheDocument();
    expect(screen.getByText("Based on 3 rapid games")).toBeInTheDocument();
    for (const el of leaves(container)) {
      expect(el.textContent ?? "").not.toMatch(/NaN|%/);
    }
    expect(container.querySelector('[data-part="band"]')).toBeNull();
  });

  it("says one more game in the singular", () => {
    render(
      <EstimateKpi
        label="Blunders"
        estimate={{ kind: "insufficient", n: 9, needed: 1 }}
        format="decimal"
        basisClass="analysed"
      />,
    );
    expect(screen.getByText("Need 1 more game")).toBeInTheDocument();
    expect(screen.getByText("Based on 9 analysed games")).toBeInTheDocument();
  });

  it("renders a percent estimate divided by 100, with its band in words", () => {
    const { container } = render(
      <EstimateKpi
        label="Accuracy"
        estimate={{
          kind: "estimate",
          value: 62.4,
          interval: { low: 60.1, high: 64.9, level: 0.8 },
          n: 50,
        }}
        format="percent"
        basisClass="analysed"
      />,
    );
    expect(container.textContent).toContain("62.4%");
    expect(container.textContent).toContain("80% likely between");
    expect(container.textContent).toContain("80% likely between 60.1% and 64.9%");
  });

  it("shows a value with no band when no interval can be estimated, never a widened one", () => {
    const { container } = render(
      <EstimateKpi
        label="Mistakes"
        estimate={{ kind: "estimate", value: 1, interval: null, n: 12 }}
        format="decimal"
        basisClass="blitz"
      />,
    );
    expect(container.textContent).toContain("1.00");
    expect(container.textContent).toContain(
      "Every game gave the same value, so there is no interval",
    );
    expect(container.textContent).not.toContain("likely between");
    expect(screen.getByText("Based on 12 blitz games")).toBeInTheDocument();
  });

  it("keeps the basis for screen readers only when the section already states it", () => {
    render(
      <EstimateKpi
        label="Blunders"
        estimate={{
          kind: "estimate",
          value: 2.4,
          interval: { low: 2.2, high: 2.6, level: 0.8 },
          n: 40,
        }}
        format="decimal"
        basisClass="analysed"
        sectionGames={40}
      />,
    );
    expect(screen.getByText("Based on 40 analysed games")).toHaveClass("sr-only");
  });

  it("shows a basis visibly when it differs from the section's", () => {
    render(
      <EstimateKpi
        label="Rapid accuracy"
        estimate={{
          kind: "estimate",
          value: 61.5,
          interval: { low: 59.2, high: 63.9, level: 0.8 },
          n: 38,
        }}
        format="percent"
        basisClass="rapid"
        sectionGames={40}
      />,
    );
    expect(screen.getByText("Based on 38 rapid games")).not.toHaveClass("sr-only");
  });

  it("below the minimum on the section's basis, leaves basis and shortfall to the section", () => {
    const { container } = render(
      <EstimateKpi
        label="Blunders"
        estimate={{ kind: "insufficient", n: 1, needed: 9 }}
        format="decimal"
        basisClass="analysed"
        sectionGames={1}
      />,
    );
    // Screen readers still get both; nothing is visible but the label and a dash.
    expect(screen.getByText("Based on 1 analysed game")).toHaveClass("sr-only");
    expect(screen.getByText("Need 9 more games")).toHaveClass("sr-only");
    expect(container.querySelector('[data-kpi="insufficient"]')?.textContent).toContain("–");
  });

  it("below the minimum on another basis, states both itself", () => {
    render(
      <EstimateKpi
        label="Rapid accuracy"
        estimate={{ kind: "insufficient", n: 2, needed: 8 }}
        format="percent"
        basisClass="rapid"
        sectionGames={3}
      />,
    );
    expect(screen.getByText("Based on 2 rapid games")).not.toHaveClass("sr-only");
    expect(screen.getByText("Need 8 more games")).not.toHaveClass("sr-only");
  });

  it("draws no ruler: the overview has no honest reference range for it", () => {
    const { container } = render(
      <EstimateKpi
        label="Blunders"
        estimate={{
          kind: "estimate",
          value: 2.4,
          interval: { low: 2.2, high: 2.6, level: 0.8 },
          n: 40,
        }}
        format="decimal"
        basisClass="analysed"
      />,
    );
    expect(container.querySelector('[data-part="band"]')).toBeNull();
  });
});

const row = (over: Partial<GameRow> = {}): GameRow => ({
  id: "g1",
  accountId: "a1",
  platform: "chesscom",
  playedAt: new Date(Date.UTC(2026, 10, 12, 18)),
  timeClass: "rapid",
  baseSeconds: 600,
  incrementSeconds: 0,
  rated: true,
  playerColor: "white",
  player: { name: "someplayer", rating: 1500 },
  opponent: { name: "Opponent", rating: 1520 },
  result: "win",
  termination: null,
  eco: null,
  openingName: null,
  analysisStatus: "done",
  accuracy: 71.3,
  counts: { blunders: 0, mistakes: 1, inaccuracies: 2 },
  plies: 60,
  ...over,
});

describe("gameCaption", () => {
  it("reads date, result, opponent with rating, and accuracy", () => {
    expect(gameCaption(row())).toBe("12 Nov 2026, win v Opponent (1520), accuracy 71.3%");
  });

  it("leaves out the accuracy when there is none, and the rating when unknown", () => {
    expect(gameCaption(row({ accuracy: null, opponent: { name: "Opponent", rating: null } }))).toBe(
      "12 Nov 2026, win v Opponent",
    );
  });
});
