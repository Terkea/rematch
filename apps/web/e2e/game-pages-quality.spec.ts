import { expect, type Page, test } from "@playwright/test";
import { adultPage, seedFixtures } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// The quality loop (M4 Task 28's checks) over the game pages: /app, /app/games and a review, on
// the e2e-fixtures games (synthetic and public fixture data). At 360px nothing scrolls sideways;
// tabbing from the top reaches the page's main content with a visible focus outline on every
// stop; and no honesty-breaking text (NaN, Infinity, undefined, null) is on the page. The axe
// check of Task 28 waits for its dependency (@axe-core/playwright), which Task 28 adds.

async function settle(page: Page, url: string, ready: string): Promise<void> {
  await gotoSettled(page, url);
  await page.waitForFunction(
    (sel) => {
      const el = document.querySelector(sel);
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    ready,
    { timeout: 180_000 },
  );
}

test("the game pages: no sideways scroll at 360px, visible focus, no broken numbers", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(600_000);
  const page = await adultPage(browser, request);
  const { analysedId, pendingId, longId } = seedFixtures();
  const routes = [
    { url: "/app", ready: "nav[aria-labelledby='tc-label'] a", main: "Overview" },
    { url: "/app/games", ready: "section[aria-label='Filters'] button", main: "Games" },
    { url: `/app/games/${analysedId}?ply=62`, ready: "[data-review]", main: "review" },
    { url: `/app/games/${pendingId}`, ready: "[data-review]", main: "pending review" },
    { url: `/app/games/${longId}?ply=300`, ready: "[data-review]", main: "300-ply review" },
  ];

  for (const width of [360, 1440]) {
    await page.setViewportSize({ width, height: width === 360 ? 800 : 900 });
    for (const r of routes) {
      await settle(page, r.url, r.ready);
      const where = `${r.main} at ${width}`;

      // Against the client width, so a classic scrollbar can't hide 15px of overflow.
      const over = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(over, `${where}: sideways overflow`).toBeLessThanOrEqual(0);

      // innerText, not textContent: what is rendered visible, not sr-only or hidden templates.
      // biome-ignore lint/performance/useDomNodeTextContent: the visible text is the point
      const text = await page.locator("main").innerText();
      expect(text, `${where}: text`).not.toMatch(/\b(NaN|Infinity|undefined|null)\b/);

      // Tab from the top: every stop shows an outline, and the walk enters <main>.
      await page.evaluate(() => {
        (document.activeElement as HTMLElement | null)?.blur();
        window.scrollTo(0, 0);
      });
      let reachedMain = false;
      for (let i = 0; i < 25 && !reachedMain; i++) {
        await page.keyboard.press("Tab");
        const stop = await page.evaluate(() => {
          const el = document.activeElement as HTMLElement | null;
          if (!el || el === document.body) return null;
          const s = getComputedStyle(el);
          const focusedShadow = s.boxShadow;
          const outline = s.outlineStyle !== "none" && Number.parseFloat(s.outlineWidth) > 0;
          // A board square draws its focus ring as an SVG child shown on :focus-visible
          // (DESIGN.md, "Focus"): a descendant that only displays while focused counts too.
          const drawn = [...el.querySelectorAll("[class*='group-focus-visible:']")].some(
            (d) => getComputedStyle(d).display !== "none",
          );
          // A shadow counts only if focus changed it (review-t13 M1: a decorative shadow is not
          // a focus ring). Blur to read the resting shadow, then put focus back where Tab left it.
          el.blur();
          const restingShadow = getComputedStyle(el).boxShadow;
          el.focus({ preventScroll: true });
          return {
            name: `${el.tagName.toLowerCase()} ${el.getAttribute("aria-label") ?? el.textContent?.trim().slice(0, 30)}`,
            outline,
            ring: (focusedShadow !== "none" && focusedShadow !== restingShadow) || drawn,
            inMain: el.closest("main") !== null,
          };
        });
        if (!stop) continue;
        expect(stop.outline || stop.ring, `${where}: focus on ${stop.name}`).toBe(true);
        reachedMain = stop.inMain;
      }
      expect(reachedMain, `${where}: tabbing reaches the page`).toBe(true);
    }
  }
  await page.context().close();
});
