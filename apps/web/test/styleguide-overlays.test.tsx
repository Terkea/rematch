// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "next-themes";
import { beforeAll, describe, expect, it } from "vitest";
import { OverlaysSection } from "../src/app/styleguide/sections/overlays.js";

beforeAll(() => {
  Element.prototype.scrollIntoView ??= () => {};
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  window.matchMedia ??= ((q: string) => ({
    matches: false,
    media: q,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
  })) as unknown as typeof window.matchMedia;
});

async function menuIn(theme: "light" | "dark") {
  const user = userEvent.setup();
  render(
    <ThemeProvider attribute="data-theme" forcedTheme={theme}>
      <OverlaysSection />
    </ThemeProvider>,
  );
  await user.click(screen.getByRole("button", { name: "Search pages and actions" }));
}

describe("styleguide command menu", () => {
  it("offers day study while the page is in night study", async () => {
    await menuIn("dark");
    expect(await screen.findByRole("option", { name: "Switch to day study" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Switch to night study" })).toBeNull();
  });

  it("offers night study while the page is in day", async () => {
    await menuIn("light");
    expect(
      await screen.findByRole("option", { name: "Switch to night study" }),
    ).toBeInTheDocument();
  });
});
