// @vitest-environment jsdom
import { render, screen, within } from "@testing-library/react";
import { beforeAll, describe, expect, it } from "vitest";
import { HeroSection } from "../src/app/styleguide/sections/hero.js";

beforeAll(() => {
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  // Reduced motion, so the hero renders its end state at once.
  window.matchMedia = ((q: string) => ({
    matches: q.includes("prefers-reduced-motion: reduce"),
    media: q,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
  })) as unknown as typeof window.matchMedia;
});

describe("styleguide hero", () => {
  it("titles the real game, not a display headline", () => {
    render(<HeroSection />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "DrNykterstein v Night-King96",
    );
  });

  it("opens the excerpt on the player's worst blunder and stops before the next one", () => {
    const { container } = render(<HeroSection />);
    const table = within(container.querySelector("table") as HTMLElement);
    const numbers = table.getAllByRole("rowheader").map((th) => th.textContent);
    expect(numbers).toEqual(["28", "29", "30"]);
    expect(table.getAllByRole("row")[0]).toHaveTextContent("Nd2??");
    expect(container.querySelector("table")).not.toHaveTextContent("Nf4??");
  });

  it("ends, under reduced motion, with the move played and the comment", () => {
    const { container } = render(<HeroSection />);
    expect(container.querySelector(`[aria-label*="after 28. Nd2??"]`)).not.toBeNull();
    const comment = container.querySelector("[data-comment]");
    expect(comment).toHaveTextContent("+2.7 became −1.1");
    expect(comment?.className).toContain("opacity-100");
  });
});
