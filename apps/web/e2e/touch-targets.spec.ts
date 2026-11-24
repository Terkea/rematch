import { expect, type Locator, type Page, test } from "@playwright/test";

// A real hit test of the 44px coarse-pointer targets from @ci/ui/primitives (DESIGN.md, "Touch
// targets"): in a touch context, document.elementFromPoint across each control's 44px band must
// land on that control. Class assertions can't see clipping by a scrolling ancestor; this can.

test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });

const BAND = 44;

/**
 * Every whole-pixel point in the control's band that misses it. The band is 44px (or the control's
 * own size, if larger) centred on the control, in both directions. Points are whole pixels because
 * Chrome rounds elementFromPoint coordinates, so a half-pixel probe can land on a neighbour.
 *
 * Rows: every pixel row of the band. Columns: for a control narrower than 44px, every column of
 * its 44px band; for a wider one, its first, middle and last columns inside the border (above and
 * below the control, the hit area is as wide as its padding box; the 1-2px border columns are the
 * control's own pixels only within its height).
 */
async function missesInBand(page: Page, target: Locator): Promise<string[]> {
  const handle = await target.elementHandle();
  if (!handle) throw new Error("target not found");
  return page.evaluate(
    ([el, band]) => {
      const r = el.getBoundingClientRect();
      const style = getComputedStyle(el);
      const span = (start: number, end: number) => [Math.ceil(start), Math.ceil(end) - 1] as const;
      const [top, bottom] = span(
        r.top + r.height / 2 - Math.max(band, r.height) / 2,
        r.top + r.height / 2 + Math.max(band, r.height) / 2,
      );
      let columns: number[];
      if (r.width < band) {
        const [left, right] = span(
          r.left + r.width / 2 - band / 2,
          r.left + r.width / 2 + band / 2,
        );
        columns = Array.from({ length: right - left + 1 }, (_, i) => left + i);
      } else {
        const [left, right] = span(
          r.left + Number.parseFloat(style.borderLeftWidth),
          r.right - Number.parseFloat(style.borderRightWidth),
        );
        columns = [left, Math.round((left + right) / 2), right];
      }
      const misses: string[] = [];
      for (const x of columns) {
        for (let y = top; y <= bottom; y++) {
          const hit = document.elementFromPoint(x, y);
          if (hit && (hit === el || el.contains(hit))) continue;
          const slot = hit?.getAttribute("data-slot");
          const who = hit ? `${hit.tagName.toLowerCase()}${slot ? `[${slot}]` : ""}` : "nothing";
          misses.push(`(${x}, ${y}) hit ${who}`);
        }
      }
      // Guard against a vacuous pass.
      if (bottom - top + 1 < band) misses.push(`band is only ${bottom - top + 1} rows`);
      return misses;
    },
    [handle, BAND] as const,
  );
}

test.beforeEach(async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "e2e", "touch emulation runs once, in the e2e project");
  await page.goto("/lab/touch-targets");
  expect(await page.evaluate(() => matchMedia("(pointer: coarse)").matches)).toBe(true);
});

test("md and sm buttons hit across their full 44px band", async ({ page }) => {
  for (const kind of ["md", "sm"]) {
    const targets = page.locator(`[data-target="${kind}"]`);
    const count = await targets.count();
    expect(count).toBeGreaterThan(1);
    for (let i = 0; i < count; i++) {
      const target = targets.nth(i);
      const box = await target.boundingBox();
      expect(box?.height).toBe(kind === "md" ? 40 : 32);
      expect(await missesInBand(page, target), `${kind} button ${i}`).toEqual([]);
    }
  }
});

test("tab triggers hit across their full 44px band, and the track never scrolls vertically", async ({
  page,
}) => {
  const scroll = page.locator('[data-slot="tabs-scroll"]');
  const { scrollHeight, clientHeight } = await scroll.evaluate((el) => ({
    scrollHeight: el.scrollHeight,
    clientHeight: el.clientHeight,
  }));
  expect.soft(scrollHeight, "tabs-scroll must not scroll vertically").toBe(clientHeight);

  const tabs = page.getByRole("tab");
  expect(await tabs.count()).toBe(3);
  for (let i = 0; i < 3; i++) {
    const tab = tabs.nth(i);
    expect((await tab.boundingBox())?.height).toBe(32);
    expect(await missesInBand(page, tab), `tab ${i}`).toEqual([]);
  }
});

test("the dialog close hits across its full 44px band", async ({ page }) => {
  await page.getByRole("button", { name: "Disconnect" }).click();
  const close = page.getByRole("button", { name: "Close" });
  await expect(close).toBeVisible();
  // Let the entrance settle, so the band is measured where the close comes to rest.
  await page.waitForTimeout(400);
  const box = await close.boundingBox();
  expect(box?.width).toBe(32);
  expect(await missesInBand(page, close)).toEqual([]);
});

test("scoresheet moves are 44px rows that hit across their whole band", async ({ page }) => {
  const sheet = page.locator('[data-fixture="scoresheet"]');
  const moves = sheet.locator('[id^="ply-"]');
  expect(await moves.count()).toBe(16);
  // Every row of the band is covered by a move of the same row (or its neighbour's own row, never a
  // dead strip): check each move's own 44px band, which equals its height.
  for (let i = 0; i < 16; i++) {
    const move = moves.nth(i);
    await move.scrollIntoViewIfNeeded();
    expect((await move.boundingBox())?.height).toBe(44);
    expect(await missesInBand(page, move), `move ${i + 1}`).toEqual([]);
  }
  // Rows touch: the next row's move starts within the 1px ruling of the previous one.
  const a = await moves.nth(0).boundingBox();
  const c = await moves.nth(2).boundingBox();
  expect((c?.y ?? 0) - ((a?.y ?? 0) + (a?.height ?? 0))).toBeLessThanOrEqual(1);
});

test("the coach states' Try again hits across its full 44px band (M5b T3 parked item)", async ({
  page,
}) => {
  test.setTimeout(300_000);
  await page.goto("/styleguide?render=all");
  const retry = page.locator("[data-section='coach']").getByRole("button", { name: "Try again" });
  await expect(retry.first()).toBeVisible({ timeout: 180_000 });
  const count = await retry.count();
  for (let i = 0; i < count; i++) {
    await retry.nth(i).scrollIntoViewIfNeeded();
    expect(await missesInBand(page, retry.nth(i)), `Try again ${i}`).toEqual([]);
  }
});
