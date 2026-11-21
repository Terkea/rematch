import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type Annotation, AnnotationLayer } from "../src/annotations/index.js";
import { Board } from "../src/board/index.js";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function stubReducedMotion(reduce: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: reduce && query.includes("reduce"),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  }));
}

const blunder: Annotation[] = [
  { kind: "arrow", from: "e4", to: "e6", tone: "pen" },
  { kind: "mark", square: "e6", glyph: "??", tone: "pen" },
];

const all: Annotation[] = [
  { kind: "circle", square: "e6", tone: "pen" },
  { kind: "arrow", from: "e4", to: "e6", tone: "pen" },
  { kind: "arrow", from: "g1", to: "f3", tone: "good" },
  { kind: "circle", square: "f7", tone: "good" },
  { kind: "arrow", from: "a2", to: "a4", tone: "pencil" },
  { kind: "mark", square: "e6", glyph: "??", tone: "pen" },
];

describe("AnnotationLayer", () => {
  it("is an aria-hidden SVG in board units", () => {
    const { container } = render(<AnnotationLayer annotations={blunder} orientation="white" />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("viewBox", "0 0 8 8");
    expect(svg).toHaveAttribute("aria-hidden", "true");
  });

  it("describes each mark in a visually hidden list", () => {
    const { container } = render(<AnnotationLayer annotations={blunder} orientation="white" />);
    const items = [...container.querySelectorAll("ul li")].map((li) => li.textContent);
    expect(items).toEqual(["Red arrow from e4 to e6", "Red ?? on e6"]);
    expect(container.querySelector("ul")).toHaveClass("sr-only");
  });

  it("names good marks green and circles by their square", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[{ kind: "circle", square: "f7", tone: "good" }]}
        orientation="black"
      />,
    );
    expect(container.querySelector("li")).toHaveTextContent("Green circle on f7");
  });

  it("renders nothing for no annotations", () => {
    const { container } = render(<AnnotationLayer annotations={[]} orientation="white" />);
    expect(container).toBeEmptyDOMElement();
  });

  it("draws every stroke twice, the sheet halo before the fill", () => {
    const { container } = render(
      <AnnotationLayer annotations={all} orientation="white" animate={false} />,
    );
    const strokes = [...container.querySelectorAll<SVGPathElement>("path[data-part]")];
    const halos = strokes.filter((p) => p.dataset.part === "halo");
    const fills = strokes.filter((p) => p.dataset.part === "fill");
    expect(halos.length).toBeGreaterThan(0);
    expect(halos).toHaveLength(fills.length);
    for (const halo of halos) expect(halo).toHaveAttribute("stroke", "var(--color-sheet)");
    // Each fill has a halo with the same outline that comes before it in paint order.
    for (const fill of fills) {
      const index = strokes.indexOf(fill);
      const halo = strokes.find(
        (p) => p.dataset.part === "halo" && p.getAttribute("d") === fill.getAttribute("d"),
      );
      expect(halo).toBeDefined();
      expect(strokes.indexOf(halo as SVGPathElement)).toBeLessThan(index);
    }
  });

  it("colours each tone with its token", () => {
    const { container } = render(
      <AnnotationLayer annotations={all} orientation="white" animate={false} />,
    );
    const fills = [...container.querySelectorAll("path[data-part='fill']")].map((p) =>
      p.getAttribute("fill"),
    );
    expect(new Set(fills)).toEqual(
      new Set(["var(--color-pen)", "var(--color-good)", "var(--color-pencil)"]),
    );
  });

  it("renders every stroke fully drawn under reduced motion", () => {
    stubReducedMotion(true);
    const { container } = render(<AnnotationLayer annotations={all} orientation="white" />);
    const reveals = [...container.querySelectorAll("mask path")];
    expect(reveals.length).toBeGreaterThan(0);
    for (const p of reveals) {
      // pathLength 1: the dash covers the whole path and starts at its start.
      expect(parseFloat(p.getAttribute("stroke-dashoffset") ?? "NaN")).toBe(0);
      expect(p.getAttribute("stroke-dasharray")).toBe("1 1");
      expect((p as SVGElement).style.opacity).not.toBe("0");
    }
  });

  it("renders every stroke fully drawn with animate={false}", () => {
    stubReducedMotion(false);
    const { container } = render(
      <AnnotationLayer annotations={all} orientation="white" animate={false} />,
    );
    for (const p of container.querySelectorAll("mask path")) {
      expect(parseFloat(p.getAttribute("stroke-dashoffset") ?? "NaN")).toBe(0);
      expect(p.getAttribute("stroke-dasharray")).toBe("1 1");
    }
  });

  it("starts strokes undrawn when it animates", () => {
    stubReducedMotion(false);
    const { container } = render(<AnnotationLayer annotations={blunder} orientation="white" />);
    const reveals = [...container.querySelectorAll("mask path")];
    expect(reveals.length).toBeGreaterThan(0);
    // pathLength 0: an empty dash, hidden until its stroke begins (a round cap would show a dot).
    for (const p of reveals) {
      expect(p.getAttribute("stroke-dasharray")).toBe("0 1");
      expect((p as SVGElement).style.opacity).toBe("0");
    }
  });

  it("stops a shaft on a ringed target's outer edge", () => {
    const shaftTipY = (annotations: Annotation[]) => {
      const { container, unmount } = render(
        <AnnotationLayer annotations={annotations} orientation="white" animate={false} />,
      );
      const shaft = container.querySelector("[data-mark='arrow'] mask path");
      const d = shaft?.getAttribute("d") ?? "";
      unmount();
      return Number(
        d
          .trim()
          .split(/[\s,A-Z]+/)
          .filter(Boolean)
          .at(-1),
      );
    };
    const bare = shaftTipY([{ kind: "arrow", from: "e4", to: "e6", tone: "pen" }]);
    const ringed = shaftTipY([
      { kind: "arrow", from: "e4", to: "e6", tone: "pen" },
      { kind: "circle", square: "e6", tone: "pen" },
    ]);
    expect(bare).toBeCloseTo(2.5 + 0.16, 3);
    expect(ringed).toBeCloseTo(2.5 + 0.46, 3);
  });

  it("moves the badge to the top-left when its arrow arrives from the right", () => {
    const badgeX = (from: "c3" | "g3") => {
      const { container, unmount } = render(
        <AnnotationLayer
          annotations={[
            { kind: "arrow", from, to: "e5", tone: "pen" },
            { kind: "mark", square: "e5", glyph: "?", tone: "pen" },
          ]}
          orientation="white"
          animate={false}
        />,
      );
      const x = Number(container.querySelector("[data-mark='badge'] circle")?.getAttribute("cx"));
      unmount();
      return x;
    };
    expect(badgeX("c3")).toBeGreaterThan(4.5);
    expect(badgeX("g3")).toBeLessThan(4.5);
  });

  it("places the badge by the leg an arrow arrives on, and by every arrow into the square", () => {
    const badgeX = (annotations: Annotation[]) => {
      const { container, unmount } = render(
        <AnnotationLayer annotations={annotations} orientation="white" animate={false} />,
      );
      const x = Number(container.querySelector("[data-mark='badge'] circle")?.getAttribute("cx"));
      unmount();
      return x;
    };
    const mark: Annotation = { kind: "mark", square: "f4", glyph: "??", tone: "pencil" };
    // h5 to f4: the long leg runs along the fifth rank, so the L arrives on f4 from above.
    expect(badgeX([{ kind: "arrow", from: "h5", to: "f4", tone: "pencil" }, mark])).toBeGreaterThan(
      5.5,
    );
    // A second arrow from the right sends the badge to the top-left corner.
    expect(
      badgeX([
        { kind: "arrow", from: "h5", to: "f4", tone: "pencil" },
        { kind: "arrow", from: "h4", to: "f4", tone: "good" },
        mark,
      ]),
    ).toBeLessThan(5.5);
  });

  it("lets a ring's overrun ink over its start without a halo cut", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[{ kind: "circle", square: "d4", tone: "pen" }]}
        orientation="white"
        animate={false}
      />,
    );
    const parts = [...container.querySelectorAll("[data-mark='circle'] path[data-part]")].map((p) =>
      p.getAttribute("data-part"),
    );
    expect(parts).toEqual(["halo", "halo", "halo", "fill", "fill", "fill"]);
  });

  it("keeps a badge off an a-file rank number by dropping it to the bottom corner", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[
          { kind: "arrow", from: "c5", to: "a5", tone: "pen" },
          { kind: "mark", square: "a5", glyph: "??", tone: "pen" },
        ]}
        orientation="white"
        animate={false}
      />,
    );
    const cy = Number(container.querySelector("[data-mark='badge'] circle")?.getAttribute("cy"));
    // a5 spans y 3 to 4: the badge sits in its lower half.
    expect(cy).toBeGreaterThan(3.5);
  });
});

describe("Board with annotations", () => {
  it("renders the layer above the pieces and below the square grid", () => {
    const { container } = render(
      <Board
        fen="rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        label="After 1. e4"
        annotations={blunder}
        animateAnnotations={false}
      />,
    );
    const layer = container.querySelector("[data-annotations]");
    expect(layer).not.toBeNull();
    const grid = container.querySelector("[role='group']");
    expect(grid).not.toBeNull();
    // Later in document order paints on top.
    const order = layer?.compareDocumentPosition(grid as Node) ?? 0;
    expect(order & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const pieces = container.querySelector("[data-board] [aria-hidden='true']");
    expect(
      (pieces?.compareDocumentPosition(layer as Node) ?? 0) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});
