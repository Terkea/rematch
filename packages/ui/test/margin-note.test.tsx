import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CrayonStroke, MarginNote, PencilTexture } from "../src/brand/index.js";

afterEach(cleanup);

describe("MarginNote", () => {
  it("renders its text in dad's handwriting", () => {
    const { container } = render(<MarginNote>same knight again</MarginNote>);
    const note = container.querySelector("[data-margin-note]");
    expect(note).toHaveTextContent("same knight again");
    expect(note?.querySelector(".font-hand")).toHaveTextContent("same knight again");
  });

  it("is hidden from assistive tech without a label, because it repeats visible text", () => {
    const { container } = render(<MarginNote>there you go</MarginNote>);
    const note = container.querySelector("[data-margin-note]");
    expect(note).toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByRole("note")).toBeNull();
  });

  it("with a label, is a note named by the label, and the handwriting itself is hidden", () => {
    const { container } = render(
      <MarginNote label="You played the same knight move again">same knight again</MarginNote>,
    );
    const note = screen.getByRole("note", { name: "You played the same knight move again" });
    expect(note).not.toHaveAttribute("aria-hidden");
    expect(container.querySelector(".font-hand")).toHaveAttribute("aria-hidden", "true");
  });

  it("draws a crayon stroke only when it celebrates", () => {
    const plain = render(<MarginNote>noted</MarginNote>);
    expect(plain.container.querySelector("[data-crayon-stroke]")).toBeNull();
    plain.unmount();
    const { container } = render(<MarginNote celebrate>there you go</MarginNote>);
    expect(container.querySelector("[data-crayon-stroke]")).not.toBeNull();
  });

  it("is never red: no pen colour on the note", () => {
    const { container } = render(<MarginNote celebrate>there you go</MarginNote>);
    expect(container.innerHTML).not.toMatch(/pen/);
  });
});

describe("CrayonStroke", () => {
  it("is decorative and drawn in the crayon token", () => {
    const { container } = render(<CrayonStroke />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelector("path")?.getAttribute("stroke")).toBe("var(--color-crayon)");
  });

  it("wavers the same way on every render (seeded)", () => {
    const a = render(<CrayonStroke seed="drill" />)
      .container.querySelector("path")
      ?.getAttribute("d");
    cleanup();
    const b = render(<CrayonStroke seed="drill" />)
      .container.querySelector("path")
      ?.getAttribute("d");
    cleanup();
    const c = render(<CrayonStroke seed="milestone" />)
      .container.querySelector("path")
      ?.getAttribute("d");
    expect(a).toBe(b);
    expect(a).not.toBe(c);
  });
});

describe("PencilTexture", () => {
  it("is a decorative, seeded graphite grain at low opacity", () => {
    const { container } = render(<PencilTexture />);
    const svg = container.querySelector("svg");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    expect(container.querySelector("feTurbulence")).toHaveAttribute("seed");
    const opacity = Number(svg?.getAttribute("opacity"));
    expect(opacity).toBeGreaterThan(0);
    expect(opacity).toBeLessThanOrEqual(0.06);
  });
});
