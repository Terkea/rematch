import { fileURLToPath } from "node:url";
import { expect, type Page, test } from "@playwright/test";
import { gotoSettled, hydrated } from "./support/mail";

// Visual baselines for the M4 surfaces (M4 Task 28): every `[data-section]` of /styleguide, plus
// /sign-in and /blocked, in the four visual projects (390 and 1440, light and dark, reduced
// motion). Every capture also pins its visible text (`.txt` beside the `.png`): a copy change
// moves too few pixels to fail a 0.001 ratio on its own (M5a T13), so the text check catches it.
// Baselines are made in the Linux qa container only:
//   ... run --rm --no-deps qa sh -c "cd apps/web && node node_modules/@playwright/test/cli.js \
//     test visual --project='visual-*' --update-snapshots"

const SHOT = {
  animations: "disabled",
  maxDiffPixelRatio: 0.001,
  // A tall section's 0.001 is hundreds of pixels, room for a new icon (M5a T13 hand-off): the
  // stricter of the two bounds applies.
  maxDiffPixels: 40,
  // Hides next dev's own issue badge, which is not part of the page (support/screenshot.css).
  stylePath: fileURLToPath(new URL("./support/screenshot.css", import.meta.url)),
} as const;

const SECTIONS = [
  "hero",
  "foundations",
  "primitives",
  "overlays",
  "board",
  "annotations",
  "eval-bar",
  "eval-graph",
  "charts",
  "trajectory",
  "stats",
  "timeline",
  "area-score",
  "progress",
  "review",
  "sound",
  "coach",
  "position-explorer",
  "profile",
  "focus",
] as const;

/** Visible text, one line per line, trailing spaces dropped: what a reader sees. */
async function visibleText(page: Page, selector: string): Promise<string> {
  // biome-ignore lint/performance/useDomNodeTextContent: the visible text is the point
  const text = await page.locator(selector).first().innerText();
  return `${text
    .split("\n")
    .map((l) => l.trimEnd())
    .join("\n")
    .trim()}\n`;
}

/**
 * The page as one tall viewport, not a `fullPage` capture: the charts measure their parent, and
 * Chromium's fullPage capture resizes the view under them (M5a T13). A tall viewport measures
 * once, before any capture.
 */
async function tall(page: Page): Promise<void> {
  const width = page.viewportSize()?.width ?? 1440;
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  await page.setViewportSize({ width, height });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
}

test.beforeEach(async ({ page }, info) => {
  test.skip(!info.project.name.startsWith("visual-"), "runs in the visual projects");
  test.setTimeout(600_000);
  const { colorScheme } = info.project.use;
  await page.emulateMedia({ colorScheme: colorScheme ?? "light", reducedMotion: "reduce" });
});

test("the styleguide sections look as they did", async ({ page }) => {
  await gotoSettled(page, "/styleguide");
  await hydrated(page, "[data-section='charts'] button[aria-label]");
  await tall(page);
  // The trend has drawn: its clip spans the plot (T4/T13).
  await expect
    .poll(() =>
      page.evaluate(() =>
        [...document.querySelectorAll("[data-section='charts'] svg[role='img']")].every((svg) => {
          const clip = svg.querySelector("clipPath rect");
          if (!clip) return true;
          return (
            Math.abs(Number(clip.getAttribute("width")) - svg.getBoundingClientRect().width) <= 1
          );
        }),
      ),
    )
    .toBe(true);
  // The progress demo counts up on a timer, then swaps in its KPI: capture the finished state.
  await expect(
    page.locator("[data-section='progress']").getByText("Based on 38 rapid games"),
  ).toBeVisible({ timeout: 30_000 });
  const found = await page
    .locator("[data-section]")
    .evaluateAll((els) => els.map((e) => e.getAttribute("data-section")));
  expect([...found].sort()).toEqual([...SECTIONS].sort());
  for (const name of SECTIONS) {
    const section = page.locator(`[data-section='${name}']`);
    await expect.soft(section).toHaveScreenshot(`styleguide-${name}.png`, SHOT);
    expect
      .soft(await visibleText(page, `[data-section='${name}']`))
      .toMatchSnapshot(`styleguide-${name}.txt`);
  }
});

for (const route of ["/sign-in", "/blocked"] as const) {
  test(`${route} looks as it did`, async ({ page }) => {
    await gotoSettled(page, route);
    await hydrated(page, route === "/sign-in" ? "form" : "a[href='/']");
    await tall(page);
    // From lg the board's sheet bleeds to the viewport's right edge, with no gutter strip of the
    // form's ground beside it (web polish critic I1).
    if ((page.viewportSize()?.width ?? 0) >= 1024) {
      const [right, width] = await page
        .locator("aside[data-surface='sheet']")
        .evaluate((el) => [el.getBoundingClientRect().right, window.innerWidth]);
      expect.soft(right).toBe(width);
    }
    const name = route.slice(1);
    await expect.soft(page).toHaveScreenshot(`${name}.png`, SHOT);
    expect.soft(await visibleText(page, "body")).toMatchSnapshot(`${name}.txt`);
  });
}
