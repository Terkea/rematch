import "@testing-library/jest-dom/vitest";
import { cleanup, render } from "@testing-library/react";
import { act } from "react";
import { hydrateRoot } from "react-dom/client";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type Annotation, AnnotationLayer } from "../src/annotations/index.js";
import { Board, NotationText } from "../src/board/index.js";

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

  it("renders plain, fully drawn strokes with no reveal masks under reduced motion", () => {
    stubReducedMotion(true);
    const { container } = render(<AnnotationLayer annotations={all} orientation="white" />);
    expect(container.querySelectorAll("[data-reveal]")).toHaveLength(0);
    for (const p of container.querySelectorAll("path[data-part]")) {
      expect(p).not.toHaveAttribute("mask");
      expect(p.getAttribute("stroke-dasharray")).not.toBe("0 1");
    }
  });

  it("renders plain, fully drawn strokes with animate={false}", () => {
    stubReducedMotion(false);
    const { container } = render(
      <AnnotationLayer annotations={all} orientation="white" animate={false} />,
    );
    expect(container.querySelectorAll("[data-reveal]")).toHaveLength(0);
    for (const p of container.querySelectorAll("path[data-part]")) {
      expect(p).not.toHaveAttribute("mask");
    }
  });

  it("starts strokes undrawn when it animates", () => {
    stubReducedMotion(false);
    const { container } = render(<AnnotationLayer annotations={blunder} orientation="white" />);
    const reveals = [...container.querySelectorAll("mask path")];
    expect(reveals.length).toBeGreaterThan(0);
    // pathLength 0: an empty dash, hidden until its stroke begins (a round cap would show a dot).
    for (const p of reveals) {
      expect(p).toHaveAttribute("data-reveal");
      expect(p.getAttribute("stroke-dasharray")).toBe("0 1");
      expect((p as SVGElement).style.opacity).toBe("0");
    }
  });

  it("stops a shaft on a ringed target's outer edge", () => {
    const shaftTipY = (annotations: Annotation[]) => {
      const { container, unmount } = render(
        <AnnotationLayer annotations={annotations} orientation="white" />,
      );
      // The arrow comes first, so its shaft is the first reveal mask.
      const shaft = container.querySelector("mask [data-reveal]");
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

  it("puts the badge in the corner farthest from every arriving tip (28. Nd2?? on d2)", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[
          { kind: "circle", square: "d2", tone: "pen" },
          { kind: "arrow", from: "e4", to: "d2", tone: "pen" },
          { kind: "arrow", from: "e2", to: "d2", tone: "good" },
          { kind: "mark", square: "d2", glyph: "??", tone: "pen" },
        ]}
        orientation="white"
        animate={false}
      />,
    );
    const c = container.querySelector("[data-mark='badge'] circle");
    // d2 spans x 3 to 4 and y 6 to 7. One arrow drops in from above, one comes in from the right.
    expect(Number(c?.getAttribute("cx"))).toBeLessThan(3.5);
    expect(Number(c?.getAttribute("cy"))).toBeGreaterThan(6.5);
  });

  it("moves the badge below an arrow that arrives from above (31...Nf4?? on f4)", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[
          { kind: "arrow", from: "h5", to: "f4", tone: "pencil" },
          { kind: "mark", square: "f4", glyph: "??", tone: "pencil" },
        ]}
        orientation="white"
        animate={false}
      />,
    );
    // f4 spans y 4 to 5.
    const cy = container.querySelector("[data-mark='badge'] circle")?.getAttribute("cy");
    expect(Number(cy)).toBeGreaterThan(4.5);
  });

  it("gives pencil a thinner halo than pen", () => {
    const { container } = render(
      <AnnotationLayer
        annotations={[
          { kind: "arrow", from: "a2", to: "a4", tone: "pencil" },
          { kind: "arrow", from: "h2", to: "h4", tone: "pen" },
        ]}
        orientation="white"
        animate={false}
      />,
    );
    const width = (tone: string) =>
      Number(
        container
          .querySelector(`[data-tone='${tone}'] path[data-part='halo']`)
          ?.getAttribute("stroke-width"),
      );
    expect(width("pencil")).toBeLessThan(width("pen"));
  });

  it("paints every arrow halo under the rings, so no arrowhead notches a ring", () => {
    const { container } = render(
      <AnnotationLayer annotations={all} orientation="white" animate={false} />,
    );
    const parts = [...container.querySelectorAll("path[data-part]")];
    const of = (kind: string, part: string) =>
      parts.filter(
        (p) =>
          p.closest("[data-mark]")?.getAttribute("data-mark") === kind &&
          p.getAttribute("data-part") === part,
      );
    const lastHalo = Math.max(...of("arrow", "halo").map((p) => parts.indexOf(p)));
    const firstRing = Math.min(...of("circle", "fill").map((p) => parts.indexOf(p)));
    expect(lastHalo).toBeLessThan(firstRing);
  });

  it("draws a duplicated annotation once, with no duplicate React keys", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const { container } = render(
      <AnnotationLayer
        annotations={[...blunder, ...blunder]}
        orientation="white"
        animate={false}
      />,
    );
    expect(container.querySelectorAll("li")).toHaveLength(2);
    expect(container.querySelectorAll("[data-mark='arrow']")).toHaveLength(1);
    expect(error).not.toHaveBeenCalled();
    error.mockRestore();
  });

  it("labels the hidden list", () => {
    const { container } = render(<AnnotationLayer annotations={blunder} orientation="white" />);
    expect(container.querySelector("ul")).toHaveAttribute("aria-label", "Annotations on the board");
  });

  it("hydrates the server markup without a mismatch", async () => {
    const element = <AnnotationLayer annotations={all} orientation="black" />;
    const host = document.createElement("div");
    host.innerHTML = renderToString(element);
    document.body.append(host);
    const errors: unknown[] = [];
    const error = vi.spyOn(console, "error").mockImplementation((...args) => {
      errors.push(args);
    });
    await act(async () => {
      hydrateRoot(host, element, { onRecoverableError: (e) => errors.push(e) });
    });
    expect(errors).toEqual([]);
    error.mockRestore();
    host.remove();
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

describe("NotationText side", () => {
  it("writes the player's glyph in pen and the opponent's in pencil", () => {
    const move = { moveNumber: 31, color: "black", san: "Nf4", judgement: "blunder" } as const;
    const { container } = render(
      <>
        <NotationText {...move} />
        <NotationText {...move} side="opponent" />
      </>,
    );
    const [mine, theirs] = [...container.querySelectorAll("[data-glyph]")];
    expect(mine).toHaveClass("text-pen-text");
    expect(theirs).toHaveClass("text-pencil");
    expect(theirs).not.toHaveClass("text-pen-text");
  });
});

// Owner request: a sequence of arrows (an engine line, a move and its reply) is numbered 1, 2, 3
// in move order, each badge on its own shaft and off every square's centre. Alternatives carry
// no number.
describe("numbered arrows", () => {
  const line: Annotation[] = [
    { kind: "arrow", from: "g1", to: "f3", tone: "good", step: 1 },
    { kind: "arrow", from: "b8", to: "c6", tone: "pencil", step: 2 },
    { kind: "arrow", from: "f1", to: "b5", tone: "good", step: 3 },
    { kind: "arrow", from: "e2", to: "e4", tone: "pen" },
  ];

  it("draws one badge per step, in move order, never on a square's centre", () => {
    const { container } = render(
      <AnnotationLayer annotations={line} orientation="white" animate={false} />,
    );
    const steps = [...container.querySelectorAll("[data-mark='step']")];
    expect(steps.map((g) => g.textContent)).toEqual(["1", "2", "3"]);
    expect(steps.map((g) => g.getAttribute("data-tone"))).toEqual(["good", "pencil", "good"]);
    for (const g of steps) {
      const c = g.querySelector("circle:last-of-type");
      const x = Number(c?.getAttribute("cx"));
      const y = Number(c?.getAttribute("cy"));
      const off = Math.hypot((x % 1) - 0.5, (y % 1) - 0.5);
      expect(off).toBeGreaterThanOrEqual(0.3);
    }
  });

  it("keeps each number off the arrowhead, nearer the tail, even on a one-square arrow", () => {
    // Owner: "Don't put the numbers over the tip of the arrow".
    const centre = (sq: string) => ({
      x: sq.charCodeAt(0) - 97 + 0.5,
      y: 8 - Number(sq[1]) + 0.5,
    });
    const arrows: [string, string][] = [
      ["g1", "f3"],
      ["h3", "h4"],
      ["f1", "b5"],
    ];
    const { container } = render(
      <AnnotationLayer
        annotations={arrows.map(([from, to], i) => ({
          kind: "arrow",
          from: from as never,
          to: to as never,
          tone: "good",
          step: i + 1,
        }))}
        orientation="white"
        animate={false}
      />,
    );
    const steps = [...container.querySelectorAll("[data-mark='step']")];
    expect(steps).toHaveLength(3);
    steps.forEach((g, i) => {
      const c = g.querySelector("circle:last-of-type");
      const p = { x: Number(c?.getAttribute("cx")), y: Number(c?.getAttribute("cy")) };
      const [from, to] = arrows[i] as [string, string];
      const tail = centre(from);
      const head = centre(to);
      const toTail = Math.hypot(p.x - tail.x, p.y - tail.y);
      const toHead = Math.hypot(p.x - head.x, p.y - head.y);
      const where = `arrow ${from}-${to} at ${p.x},${p.y}`;
      // Well clear of the arrowhead on every arrow (a knight's L included)…
      expect(toHead, where).toBeGreaterThan(0.6);
      // …and on a straight arrow, nearer the tail than the head.
      const straight =
        from[0] === to[0] ||
        from[1] === to[1] ||
        Math.abs(head.x - tail.x) === Math.abs(head.y - tail.y);
      if (straight) expect(toTail, where).toBeLessThan(toHead);
    });
  });

  it("names each step for screen readers, in order", () => {
    const { container } = render(
      <AnnotationLayer annotations={line} orientation="white" animate={false} />,
    );
    const items = [...container.querySelectorAll("ul li")].map((li) => li.textContent);
    expect(items).toEqual([
      "1: Green arrow from g1 to f3",
      "2: Grey arrow from b8 to c6",
      "3: Green arrow from f1 to b5",
      "Red arrow from e2 to e4",
    ]);
  });
});
