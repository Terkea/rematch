import { mkdir } from "node:fs/promises";
import { expect, type Locator, type Page, test, webkit } from "@playwright/test";
import { gotoSettled, hydrated } from "./support/mail";

// The accuracy trend in real browsers (M5a Task 4, deferred to Task 13): a keyboard step costs
// under 16 ms at a 4x CPU throttle, a tap on a phone opens the nearest game and Previous / Next
// reach its neighbours, and WebKit draws the trend (the drawIn clip, review-t4 M3). It runs on
// the styleguide's synthetic 500-game trend, so it needs no account.

const TREND = "[data-section='charts'] [data-part='hit']";

/** The styleguide's first trend: its hit layer and its dot buttons, once hydrated. */
async function trend(page: Page): Promise<{ hit: Locator; dots: Locator }> {
  await gotoSettled(page, "/styleguide");
  await hydrated(page, "[data-section='charts'] button[aria-label]");
  const hit = page.locator(TREND).first();
  await hit.scrollIntoViewIfNeeded();
  const dots = hit.locator("xpath=..").locator("button[aria-label]");
  expect(await dots.count()).toBeGreaterThan(100);
  return { hit, dots };
}

test("a keyboard step on the 500-game trend stays under 16 ms at 4x CPU throttle", async ({
  page,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  // `next dev` runs React's development build, about ten times slower per render, so the 16 ms
  // budget is only meaningful on a production build (`next build`, then `next start` with
  // LAB_ENABLED=1 for the styleguide): set E2E_PRODUCTION=1 when E2E_BASE_URL points at one.
  // CI's e2e job runs on a production build and sets it (ADR 0067), so the budget is enforced on
  // every change. M4 T28 brought the step back under it (ADR-WEB-18).
  test.skip(process.env.E2E_PRODUCTION !== "1", "needs a production build (E2E_PRODUCTION=1)");
  test.setTimeout(300_000);
  const { dots } = await trend(page);
  await dots.first().focus();
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });
  try {
    // Key to paint (review-t13 I3): from the keydown to the second animation frame after it, so
    // the handler, React's render and commit, style, layout and the paint of the frame holding
    // the change are all inside. The same wait with no key is the idle baseline: what the frame
    // clock alone costs. The key's own cost is the difference, and it must fit in one frame.
    const { keyed, idle } = await page.evaluate(async () => {
      const twoFrames = () =>
        new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const keyed: number[] = [];
      const idle: number[] = [];
      for (let i = 0; i < 40; i++) {
        await twoFrames();
        let t0 = performance.now();
        await twoFrames();
        idle.push(performance.now() - t0);
        const el = document.activeElement as HTMLElement;
        t0 = performance.now();
        el.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
        await twoFrames();
        keyed.push(performance.now() - t0);
      }
      return { keyed, idle };
    });
    const stats = (xs: number[]) => {
      const s = [...xs].sort((a, b) => a - b);
      return {
        median: s[Math.floor(s.length / 2)] as number,
        p90: s[Math.floor(s.length * 0.9)] as number,
        max: s.at(-1) as number,
      };
    };
    const k = stats(keyed);
    const i = stats(idle);
    const cost = k.median - i.median;
    const costP90 = k.p90 - i.p90;
    const f = (n: number) => n.toFixed(1);
    info.annotations.push({
      type: "timing",
      description:
        `key to paint median ${f(k.median)} p90 ${f(k.p90)} max ${f(k.max)} ms; ` +
        `idle median ${f(i.median)} p90 ${f(i.p90)} ms; ` +
        `key cost median ${f(cost)} p90 ${f(costP90)} ms`,
    });
    console.log(`trend step at 4x: ${info.annotations.at(-1)?.description}`);
    expect(cost).toBeLessThan(16);
    expect(costP90).toBeLessThan(16);
  } finally {
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  }
});

test.describe("on a touch phone", () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });

  test("a tap opens the nearest game, and Previous and Next step through games", async ({
    page,
  }, info) => {
    test.skip(info.project.name !== "e2e", "touch emulation runs once, in the e2e project");
    test.setTimeout(300_000);
    const { dots } = await trend(page);
    const n = await dots.count();
    const target = dots.nth(Math.floor(n / 2));
    const box = await target.boundingBox();
    if (!box) throw new Error("no dot box");
    const x = box.x + box.width / 2;
    const y = box.y + box.height / 2;

    // Every dot whose centre is on the tapped point is a fair winner (dense plots overlap).
    const under = await dots.evaluateAll(
      (els, [px, py]) =>
        els
          .filter((e) => {
            const r = e.getBoundingClientRect();
            return Math.hypot(r.x + r.width / 2 - px, r.y + r.height / 2 - py) < 1;
          })
          .map((e) => e.getAttribute("aria-label")),
      [x, y] as const,
    );
    await page.touchscreen.tap(x, y);
    // The styleguide also pins an open "How linking works" popover; pick the trend's one.
    const dialog = page.getByRole("dialog").filter({ hasText: "Review game" });
    await expect(dialog).toBeVisible();
    const caption = dialog.locator("p").first();
    const first = await caption.textContent();
    expect(under).toContain(first);

    await dialog.getByRole("button", { name: "Next game" }).tap();
    await expect(caption).not.toHaveText(first ?? "");
    const next = await caption.textContent();
    await dialog.getByRole("button", { name: "Previous game" }).tap();
    await expect(caption).toHaveText(first ?? "");
    expect(next).not.toBe(first);

    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    // A tap far from every dot (the 0% line, mid-plot: no game scored near 0) opens nothing.
    const plot = await page.locator(TREND).first().boundingBox();
    if (!plot) throw new Error("no plot box");
    await page.touchscreen.tap(plot.x + plot.width / 2, plot.y + 180);
    await page.waitForTimeout(300);
    await expect(dialog).toBeHidden();
  });
});

test("WebKit draws the trend and its dots", async ({ baseURL }, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const browser = await webkit.launch();
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, baseURL });
    const { hit } = await trend(page);
    const svg = hit.locator("xpath=..").locator("svg[role='img']");
    // The drawIn clip ends at the plot's full width, and the line and dots are painted.
    await expect
      .poll(async () =>
        svg.evaluate((el) => {
          const rect = el.querySelector("clipPath rect");
          const w = Number(rect?.getAttribute("width") ?? Number.NaN);
          return Math.abs(w - el.getBoundingClientRect().width) <= 1;
        }),
      )
      .toBe(true);
    expect(await svg.locator("[data-part='mean']").count()).toBeGreaterThan(0);
    expect(await svg.locator("[data-part='fill'], [data-part='half']").count()).toBeGreaterThan(
      100,
    );
    const shot = await svg.screenshot();
    // Painted, not blank: a blank capture of an SVG is one repeated colour.
    expect(shot.length).toBeGreaterThan(5_000);
    await mkdir(info.outputDir, { recursive: true });
    await svg.screenshot({ path: `${info.outputDir}/trend-webkit-1440.png` });
  } finally {
    await browser.close();
  }
});
