import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import { enterAge, gotoSettled, hydrated, signInByMagicLink } from "./support/mail";

// The games explorer on a phone (review of Task 9): no state may scroll sideways. The width is
// measured as the document's scroll width against its client width, which with classic
// scrollbars excludes the vertical bar, so a `100vw` element or an over-wide row shows up as
// overflow. The games are synthetic (e2e/support/seed-games.ts).

const EMAIL = "games@example.invalid";

async function overflow(page: Page): Promise<number> {
  return page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
}

test("the explorer never scrolls sideways at 390 or 360 px", async ({ page, request }, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(420_000);

  await signInByMagicLink(page, request, EMAIL);
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 1990, 6);
  await expect(page).toHaveURL(/\/app\/accounts$/);

  const seed = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/support/seed-games.ts", EMAIL],
    { stdio: "inherit", env: process.env },
  );
  expect(seed.status).toBe(0);

  for (const width of [390, 360]) {
    await page.setViewportSize({ width, height: 844 });
    for (const url of ["/app/games", "/app/games?page=2", "/app/games?sort=accuracy-asc"]) {
      await gotoSettled(page, url);
      await expect(page.getByRole("heading", { name: "Games", level: 1 })).toBeVisible();
      await expect(page.getByText(/^Showing /)).toBeVisible();
      expect(await overflow(page), `${url} at ${width}`).toBe(0);
    }

    // The open filter panel, with chips for the active filters.
    await gotoSettled(page, "/app/games?tc=rapid&result=loss&from=2026-10-01");
    await hydrated(page, "section[aria-label='Filters'] button");
    await page.getByRole("button", { name: /^Filters/ }).click();
    await expect(page.getByLabel("From", { exact: true })).toBeVisible();
    await expect(page.getByRole("list", { name: "Active filters" })).toBeVisible();
    expect(await overflow(page), `filters open at ${width}`).toBe(0);

    // The no-match state.
    await gotoSettled(page, "/app/games?tc=classical");
    await expect(page.getByText("No games match these filters.")).toBeVisible();
    expect(await overflow(page), `no match at ${width}`).toBe(0);
  }
});
