import { fileURLToPath } from "node:url";
import { expect, type Page, test } from "@playwright/test";
import { adultPage, seedFixtures } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// Visual baselines for the game pages (M5a Task 13, as M4 Task 28 does for the styleguide), in
// the four visual projects (390 and 1440, light and dark, reduced motion). The data is the
// e2e-fixtures seed: public fixture games and synthetic ones with fixed dates, so the pages are
// the same on every run. Baselines are made in the Linux qa container only:
//   ... run --rm --no-deps qa sh -c "cd apps/web && node node_modules/@playwright/test/cli.js \
//     test game-pages-visual --project='visual-*' --update-snapshots"

const SHOT = {
  animations: "disabled",
  maxDiffPixelRatio: 0.001,
  // On a whole tall page 0.001 is about 600 pixels: a new icon or a moved label fits under it
  // (the sound toggle and the strip's corner name both did, M4 T28). The stricter bound wins.
  maxDiffPixels: 40,
  // Hides next dev's own issue badge, which is not part of the page (support/screenshot.css).
  stylePath: fileURLToPath(new URL("./support/screenshot.css", import.meta.url)),
} as const;

/**
 * The whole page, captured as one tall viewport rather than with `fullPage`: Chromium's fullPage
 * capture resizes the view under the page, and the trend (which measures its parent) is caught
 * mid-measure and blank at 1440. A tall viewport measures once, before the capture.
 */
async function wholePage(page: Page, name: string): Promise<void> {
  const width = page.viewportSize()?.width ?? 1440;
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  await page.setViewportSize({ width, height });
  await page.waitForTimeout(500);
  await expect(page).toHaveScreenshot(name, SHOT);
  // The visible text too (M4 T28): a copy change moves too few pixels to fail a 0.001 ratio on
  // its own (M5a T13 found four such edits passing), so the text snapshot catches it.
  // biome-ignore lint/performance/useDomNodeTextContent: the visible text is the point
  const text = await page.locator("main").innerText();
  const lines = text.split("\n").map((l) => l.trimEnd());
  expect(`${lines.join("\n").trim()}\n`).toMatchSnapshot(name.replace(/\.png$/, ".txt"));
}

async function ready(page: Page, url: string, selector: string): Promise<void> {
  await gotoSettled(page, url);
  await page.waitForFunction(
    (sel) => {
      const el = document.querySelector(sel);
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    selector,
    { timeout: 180_000 },
  );
  await page.evaluate(() => document.fonts.ready);
}

test("the game pages look as they did", async ({ browser, request }, info) => {
  test.skip(!info.project.name.startsWith("visual-"), "runs in the visual projects");
  test.setTimeout(600_000);
  const page = await adultPage(browser, request);
  const { analysedId } = seedFixtures();
  const { viewport, colorScheme } = info.project.use;
  const base = viewport ?? { width: 1440, height: 900 };
  await page.setViewportSize(base);
  await page.emulateMedia({ colorScheme: colorScheme ?? "light", reducedMotion: "reduce" });

  // The overview, once the trend has drawn (T4: the capture waits for the trend SVG).
  await ready(page, "/app", "nav[aria-labelledby='tc-label'] a");
  await expect(page.locator("[data-chart='trend'] svg [data-part='mean']").first()).toBeAttached();
  const drawn = () =>
    page.evaluate(() => {
      const svg = document.querySelector("[data-chart='trend'] svg[role='img']");
      const clip = Number(svg?.querySelector("clipPath rect")?.getAttribute("width"));
      return svg !== null && Math.abs(clip - svg.getBoundingClientRect().width) <= 1;
    });
  await expect.poll(drawn).toBe(true);
  await wholePage(page, "overview.png");
  await expect.poll(drawn).toBe(true);

  await page.setViewportSize(base);
  await ready(page, "/app/games", "section[aria-label='Filters'] button");
  await wholePage(page, "games.png");

  // The review on the player's blunder (62... in the fixture), with the ?ply= that skips the
  // reveal.
  await page.setViewportSize(base);
  await ready(page, `/app/games/${analysedId}?ply=62`, "[data-review]");
  await expect(page.locator("[data-review]")).toHaveAttribute("data-reveal", "done");
  await wholePage(page, "review.png");

  await page.context().close();
});
