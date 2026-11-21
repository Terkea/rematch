// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import VariantA from "../src/app/lab/board/a/page.js";
import VariantB from "../src/app/lab/board/b/page.js";
import VariantC from "../src/app/lab/board/c/page.js";

const variants = [
  { name: "a", Page: VariantA, squares: "flat", coords: "outside-sans", stroke: "fine" },
  { name: "b", Page: VariantB, squares: "hatched", coords: "outside-serif", stroke: "marker" },
  { name: "c", Page: VariantC, squares: "well", coords: "on-squares", stroke: "tapered" },
] as const;

describe("board treatment lab", () => {
  for (const v of variants) {
    describe(`variant ${v.name}`, () => {
      it("shows the middlegame blunder with one arrow, one ring and one ?? badge", () => {
        const { container } = render(<v.Page />);
        const board = container.querySelector("[data-lab-board]");
        expect(board).not.toBeNull();
        expect(board?.getAttribute("data-ply")).toBe("55");
        expect(container.querySelectorAll('[data-mark="arrow"]')).toHaveLength(1);
        expect(container.querySelectorAll('[data-mark="ring"]')).toHaveLength(1);
        const badges = container.querySelectorAll('[data-mark="badge"]');
        expect(badges).toHaveLength(1);
        expect(badges[0]?.textContent).toBe("??");
      });

      it("differs on square treatment, coordinate placement and stroke character", () => {
        const { container } = render(<v.Page />);
        const board = container.querySelector("[data-lab-board]");
        expect(board?.getAttribute("data-squares")).toBe(v.squares);
        expect(board?.getAttribute("data-coords")).toBe(v.coords);
        expect(board?.getAttribute("data-stroke")).toBe(v.stroke);
      });

      it("uses tokens only, never raw colours, in the marks", () => {
        const { container } = render(<v.Page />);
        const marks = container.querySelector("[data-lab-marks]");
        expect(marks).not.toBeNull();
        expect(marks?.outerHTML ?? "").not.toMatch(/#[0-9a-f]{3,8}\b|rgb\(/i);
      });
    });
  }
});
