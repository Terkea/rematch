import { expect, type Page, test } from "@playwright/test";

// The styleguide's forced Hover and Focus-visible cells must look different from Rest, in the
// browser's computed style (Task 9 critic: two hover cells matched Rest pixel for pixel because the
// forcing rewrote an escaped colon inside a stacked-variant class name). Behind the lab guard.

const CONTROL =
  "button, input:not([type='checkbox']), [role='switch'], [data-slot='select-trigger']";

async function look(page: Page, row: string, state: string) {
  return page
    .locator(`[data-state-row="${row}"] [data-state="${state}"]`)
    .locator(CONTROL)
    .first()
    .evaluate((el) => {
      const s = getComputedStyle(el);
      return `${s.borderColor}|${s.backgroundColor}|${s.color}|${s.outlineStyle}`;
    });
}

for (const theme of ["light", "dark"] as const) {
  test(`every hover and focus cell differs from rest (${theme})`, async ({ page }) => {
    await page.addInitScript((t) => localStorage.setItem("theme", t), theme);
    await page.goto("/styleguide?render=all");
    await expect(page.locator("style[data-forced-states]")).toHaveCount(1);
    for (const row of [
      "Primary",
      "Outline",
      "Quiet",
      "Danger",
      "Small",
      "Text field",
      "Select",
      "Switch",
    ]) {
      const rest = await look(page, row, "rest");
      expect.soft(await look(page, row, "hover"), `${row} hover (${theme})`).not.toBe(rest);
      expect.soft(await look(page, row, "focus"), `${row} focus (${theme})`).not.toBe(rest);
    }
  });
}
