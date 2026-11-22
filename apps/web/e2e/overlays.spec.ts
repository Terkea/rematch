import { expect, type Page, test } from "@playwright/test";
import { gotoSettled, hydrated } from "./support/mail";

// The overlay checks handed from M4 Task 8 to the Task 28 sweep, in a real browser on the
// styleguide's overlays section: the drawer leaves the DOM when it closes, the dialog and drawer
// only fade (or barely travel) under reduced motion, the toast's dismiss hits across its full
// 44px band on a touch screen, and Ctrl+K never opens the menu from a textarea, a select or
// contenteditable text.

async function overlays(page: Page): Promise<void> {
  await gotoSettled(page, "/styleguide");
  await hydrated(page, "[data-section='overlays'] button");
  await page.locator("[data-section='overlays']").scrollIntoViewIfNeeded();
}

// Playwright needs a destructured fixture here; browserName is the cheapest.
test.beforeEach(({ browserName }, info) => {
  expect(browserName).toBe("chromium");
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
});

test("the drawer unmounts when it closes", async ({ page }) => {
  await overlays(page);
  await page.getByRole("button", { name: "Filters", exact: true }).click();
  const drawer = page.getByRole("dialog", { name: "Filters" });
  await expect(drawer).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(drawer).toHaveCount(0);
  await expect(page.getByText("Narrow your games before you review them.")).toHaveCount(0);
});

test.describe("under reduced motion", () => {
  test.use({ reducedMotion: "reduce" });

  test("the dialog fades in place: no scale and no travel on any frame", async ({ page }) => {
    await overlays(page);
    await page
      .locator("[data-section='overlays'] button[aria-haspopup='dialog']", {
        hasText: "Delete account",
      })
      .click();
    const frames = await page.evaluate(async () => {
      const out: { scale: string; translate: string; opacity: string }[] = [];
      for (let i = 0; i < 20; i++) {
        const el = document.querySelector("[data-slot='dialog-content']");
        if (el) {
          const s = getComputedStyle(el);
          out.push({ scale: s.scale, translate: s.translate, opacity: s.opacity });
        }
        await new Promise((r) => requestAnimationFrame(r));
      }
      return out;
    });
    expect(frames.length).toBeGreaterThan(5);
    for (const f of frames) expect(["none", "1"]).toContain(f.scale);
    expect(new Set(frames.map((f) => f.translate)).size).toBe(1);
    await page.keyboard.press("Escape");
  });

  test("the drawer travels in the instant duration", async ({ page }) => {
    await overlays(page);
    await page.getByRole("button", { name: "Filters", exact: true }).click();
    const drawer = page.getByRole("dialog", { name: "Filters" });
    await expect(drawer).toBeVisible();
    const instant = await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue("--dur-instant").trim(),
    );
    const duration = await drawer.evaluate((el) => getComputedStyle(el).transitionDuration);
    expect(instant).toBe("90ms");
    expect(duration.split(",").map((d) => d.trim())).toContain("0.09s");
    await page.keyboard.press("Escape");
  });
});

test.describe("on a touch phone", () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });

  test("a toast's dismiss hits across its full 44px band", async ({ page }) => {
    await overlays(page);
    await page.getByRole("button", { name: "Show a note" }).tap();
    const toast = page.locator("[data-sonner-toast]").filter({ hasText: "Code copied" });
    await expect(toast).toBeVisible();
    const dismiss = toast.getByRole("button", { name: "Dismiss" });
    const box = await dismiss.boundingBox();
    if (!box) throw new Error("no dismiss box");
    // The drawn button is 32px; its hit area reaches 44px. A tap 21px below its centre is
    // outside the drawn button and inside the band.
    expect(box.height).toBeLessThan(44);
    await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2 + 21);
    await expect(toast).toHaveCount(0);
  });
});

test("Ctrl+K opens the menu, but never from a textarea, a select or contenteditable", async ({
  page,
}) => {
  await overlays(page);
  await page.evaluate(() => {
    const host = document.createElement("div");
    host.id = "editables";
    host.innerHTML =
      '<textarea aria-label="Notes"></textarea>' +
      '<select aria-label="Pick"><option>One</option></select>' +
      '<div contenteditable="true" aria-label="Rich">text</div>';
    document.body.append(host);
  });
  const menu = page.getByRole("dialog").filter({ has: page.getByRole("combobox") });
  for (const sel of ["#editables textarea", "#editables select", "#editables [contenteditable]"]) {
    await page.locator(sel).focus();
    await page.keyboard.press("Control+k");
    await page.waitForTimeout(200);
    await expect(menu, `no menu from ${sel}`).toHaveCount(0);
  }
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("Control+k");
  await expect(menu).toBeVisible();
});
