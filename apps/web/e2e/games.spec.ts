import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import { adultPage, seedFixtures } from "./support/game-pages";
import { enterAge, gotoSettled, hydrated, signInByMagicLink } from "./support/mail";

// The games explorer on a phone (review of Task 9): no state may scroll sideways. The width is
// measured as the document's scroll width against its client width, which with classic
// scrollbars excludes the vertical bar, so a `100vw` element or an over-wide row shows up as
// overflow. The games are synthetic (e2e/support/seed-games.ts).

const EMAIL = "games@example.invalid";
const RIVAL = "rival@example.invalid";

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

  // The front door (M5a Task 12): with a linked account, "/" opens the overview, and the header
  // leads on to the games.
  await gotoSettled(page, "/");
  await expect(page).toHaveURL(/\/app$/);
  const nav = page.getByRole("navigation", { name: "Main" });
  await expect(nav.getByRole("link", { name: "Overview" })).toHaveAttribute("aria-current", "page");
  await nav.getByRole("link", { name: "Games" }).click();
  await expect(page).toHaveURL(/\/app\/games$/);
  await expect(page.getByRole("heading", { name: "Games", level: 1 })).toBeVisible();

  for (const width of [390, 360]) {
    await page.setViewportSize({ width, height: 844 });
    for (const url of ["/app/games", "/app/games?page=2", "/app/games?sort=accuracy-asc"]) {
      await gotoSettled(page, url);
      await expect(page.getByRole("heading", { name: "Games", level: 1 })).toBeVisible();
      await expect(page.getByText(/^Showing /)).toBeVisible();
      expect(await overflow(page), `${url} at ${width}`).toBe(0);
    }

    // The open shared filter bar (T3), its custom range open for a range that is no preset.
    await gotoSettled(page, "/app/games?tc=rapid&result=loss&from=2026-10-01");
    const phoneBar = page.locator("[data-filter-bar] > details");
    await phoneBar.locator("> summary").click();
    await expect(phoneBar.getByLabel("From", { exact: true })).toBeVisible();
    expect(await overflow(page), `filters open at ${width}`).toBe(0);

    // The no-match state.
    await gotoSettled(page, "/app/games?tc=classical");
    await expect(page.getByText("No games match these filters.")).toBeVisible();
    expect(await overflow(page), `no match at ${width}`).toBe(0);
  }
});

// The game pages end to end (M5a Task 13), as the adult e2e user on the e2e-fixtures account
// (e2e/helpers/seed-games.ts: public fixture games and synthetic ones, never real data). The
// signed-in state comes from accounts.spec.ts when it ran first; otherwise it signs in.

test("overview, explorer and review, end to end", async ({ browser, request }, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(600_000);

  const page = await adultPage(browser, request);
  const ids = seedFixtures();

  await test.step("1. the front door lands on the overview with the seeded counts", async () => {
    await gotoSettled(page, "/");
    await expect(page).toHaveURL(/\/app$/);
    await expect(page.getByRole("heading", { name: "Overview", level: 1 })).toBeVisible();
    // The compact sync panel (M5b Task P2) replaced the status line: its totals line states the
    // same counts.
    await expect(page.locator("[data-slot='sync-panel'][data-variant='compact']")).toContainText(
      "34 of 35 games analysed",
    );
  });

  await test.step("2. Rapid in the time-control switch", async () => {
    await hydrated(page, "[data-filter-bar] a");
    // The shared filter bar (T3): its desktop copy is the one shown at this width.
    const tc = page.locator("[data-filter-bar] > div").getByRole("group", { name: "Time control" });
    await tc.getByRole("link", { name: /^Rapid/ }).click();
    await expect(page).toHaveURL(/[?&]tc=rapid\b/);
    await expect(tc.getByRole("link", { name: /^Rapid/ })).toHaveAttribute("aria-current", "true");
    await expect(page.getByText(/^Across 16 rapid games your accuracy averages/)).toBeVisible();
    await expect(page.getByText(/^Across 16 rapid games\.$/)).toBeVisible();
  });

  await test.step("3. Games, filtered to losses", async () => {
    await page
      .getByRole("navigation", { name: "Main" })
      .getByRole("link", { name: "Games" })
      .click();
    await expect(page).toHaveURL(/\/app\/games$/);
    await hydrated(page, "section[aria-label='Results and sort'] [role='combobox']");
    // The list opens on rapid (I8); the analysed fixture is not rapid, so show every time control.
    await page
      .locator("[data-filter-bar] > div")
      .getByRole("group", { name: "Time control" })
      .getByRole("link", { name: /^All/ })
      .click();
    await expect(page).toHaveURL(/[?&]tc=all\b/);
    await page
      .getByRole("radiogroup", { name: "Result" })
      .getByRole("radio", { name: "Losses" })
      .click();
    await expect(page).toHaveURL(/[?&]result=loss\b/);
    const rows = page.locator("a[href^='/app/games/'][aria-label]").filter({ visible: true });
    await expect
      .poll(async () => {
        const labels = await rows.evaluateAll((els) =>
          els.map((e) => e.getAttribute("aria-label")),
        );
        return labels.length > 0 && labels.every((l) => l?.startsWith("Loss v "));
      })
      .toBe(true);
  });

  await test.step("4. a row opens its review; End, Home and three steps reach ply 3", async () => {
    const rows = page.locator("a[href^='/app/games/'][aria-label]").filter({ visible: true });
    // The newest loss is the analysed fixture. The sync panel above the list updates live (M5b
    // P2), so a coordinate click can land on a row that moved; click the row itself instead.
    await expect(rows.first()).toHaveAttribute("href", new RegExp(`/app/games/${ids.analysedId}$`));
    await rows.first().dispatchEvent("click");
    await expect(page).toHaveURL(new RegExp(`/app/games/${ids.analysedId}$`));
    await hydrated(page, "[data-review]");
    for (const key of ["End", "Home", "ArrowRight", "ArrowRight", "ArrowRight"])
      await page.keyboard.press(key);
    await expect(page.locator("[data-review]")).toHaveAttribute("data-ply", "3");
    await expect(page.locator("#ply-3")).toHaveAttribute("aria-current", "step");
    await expect(page).toHaveURL(/[?&]ply=3\b/, { timeout: 500 });
  });

  await test.step("5. the engine's next move adds one arrow", async () => {
    const arrows = page.locator("[data-annotations] g[data-mark='arrow']");
    const before = await arrows.count();
    await page.getByRole("switch", { name: "Show the engine's next move" }).click();
    await expect(arrows).toHaveCount(before + 1);
  });

  await test.step("6. a pending game says so, and its moves replay", async () => {
    await gotoSettled(page, `/app/games/${ids.pendingId}`);
    // Inside main: a production build streams the page, and React's hidden staging copy of the
    // same text sits at the end of the body until it is swapped in.
    await expect(
      page.getByRole("main").getByText("This game is waiting for analysis.", { exact: false }),
    ).toBeVisible();
    await hydrated(page, "[data-review]");
    // It opens on the final position; Home, then two steps.
    for (const key of ["Home", "ArrowRight", "ArrowRight"]) await page.keyboard.press(key);
    await expect(page.locator("[data-review]")).toHaveAttribute("data-ply", "2");
    await expect(page.locator("#ply-2")).toHaveAttribute("aria-current", "step");
  });

  await test.step("7. an unknown game id is not found", async () => {
    await gotoSettled(page, "/app/games/00000000-0000-0000-0000-000000000000");
    await expect(page.getByText("We couldn't find that game.")).toBeVisible();
  });

  await test.step("8. another user gets not found for this user's game", async () => {
    const other = await (await browser.newContext()).newPage();
    await signInByMagicLink(other, request, RIVAL);
    await expect(other).toHaveURL(/\/welcome\/age$/);
    await enterAge(other, 1985, 3);
    await gotoSettled(other, `/app/games/${ids.analysedId}`);
    await expect(other.getByText("We couldn't find that game.")).toBeVisible();
    await other.context().close();
  });

  await page.context().close();
});
