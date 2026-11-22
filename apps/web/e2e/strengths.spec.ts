import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import { adultPage } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// The strengths page (`/app/strengths`) on the synthetic strengths seed
// (e2e/helpers/seed-strengths.ts: the coach world plus strength plies on two public fixture
// games). No band reference exists, so the page compares with the player's own earlier games and
// says so; Maia is not running, so the boards are the labelled critical-moment fallback.

function seed(command?: "clear"): void {
  const run = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/helpers/seed-strengths.ts", ...(command ? [command] : [])],
    { env: process.env, encoding: "utf8" },
  );
  expect(run.status, run.stderr).toBe(0);
}

test.describe
  .serial("the strengths page", () => {
    let page: Page;

    test.beforeAll(async ({ browser, request }, info) => {
      test.skip(info.project.name !== "e2e", "runs in the e2e project");
      test.setTimeout(600_000);
      page = await adultPage(browser, request);
      seed();
    });

    test.afterAll(async () => {
      seed("clear");
      await page?.context().close();
    });

    test("the nav opens it after Weaknesses, rapid first, each section honest", async () => {
      test.setTimeout(300_000);
      await gotoSettled(page, "/app/weaknesses");
      const nav = page.getByRole("navigation", { name: "Main" });
      const labels = await nav.getByRole("link").allTextContents();
      expect(labels.indexOf("Strengths")).toBe(labels.indexOf("Weaknesses") + 1);
      await nav.getByRole("link", { name: "Strengths" }).click();
      await page.waitForURL(/\/app\/strengths$/);
      await expect(page.getByRole("heading", { level: 1 })).toHaveText("Strengths");
      await expect(
        page.getByText(/^Based on 15 analysed rapid games on chess\.com\./),
      ).toBeVisible();

      const above = page.locator('[data-section="above"]');
      await expect(
        above.getByText(
          "Compared with your own earlier games, since there's no benchmark for your level yet.",
        ),
      ).toBeVisible();
      await expect(above.getByText(/^Not enough games yet\./)).toBeVisible();

      // W8.basics' improving transition: p_change 0.85, 14.0% to 9.0% of chances.
      const improving = page.locator('[data-section="improving"]');
      await expect(improving.locator('[data-claim="W8.basics"]')).toContainText(
        "14.0% of chances before, 9.0% after, detected improving on 15 Oct 2026 (85% likely a real change).",
      );
      await expect(page.locator('[data-section="openings"]')).toContainText("Not enough games yet");
      await expect(page.locator("[data-report-strengths]")).toContainText(
        "Solid Italian repertoire",
      );
      await expect(page.locator("main")).not.toContainText(/NaN|undefined|Infinity/);
    });

    test("blitz shows the critical-moment fallback in explorers that open the game", async () => {
      test.setTimeout(300_000);
      await gotoSettled(page, "/app/strengths?tc=blitz");
      const hard = page.locator('[data-section="hard-spots"]');
      // No Maia rating: the fallback's title never claims the moves were hard (N11).
      await expect(hard.getByRole("heading", { level: 2 })).toHaveText(
        "Critical moments you got right",
      );
      await expect(hard).toContainText("Difficulty pending: Maia hasn't rated these positions yet");
      await expect(hard).toContainText("Until it does, these are critical moments you got right");
      const spots = hard.locator("[data-spot]");
      await expect(spots).toHaveCount(3);
      // 10… Qxf4 took a free piece after White's blunder: left out despite the biggest gap.
      await expect(hard).not.toContainText("Qxf4");
      // One per game first, biggest gap first: 8… Qb6, then 27… Rf8.
      await expect(spots.nth(0)).toContainText(
        "The engine's next choice was 31 win% points worse.",
      );
      await expect(spots.nth(0)).toContainText("Keep doing this: on every move, look at your own");
      await expect(spots.nth(1)).toContainText("27… Rf8");

      // Each board is a position explorer (N14): it expands into the game around the move.
      const dialog = page.getByRole("dialog", { name: "Position before 8… Qb6" });
      // The explorer is a client island: retry the click until it has hydrated.
      await expect(async () => {
        await spots
          .nth(0)
          .getByRole("button", { name: "Expand the position before 8… Qb6" })
          .click();
        await expect(dialog).toBeVisible({ timeout: 2_000 });
      }).toPass({ timeout: 60_000 });
      await expect(dialog.locator("[data-ply='16']")).toBeVisible();
      await expect(dialog.locator("[data-ply='15']")).toHaveAttribute("aria-current", "step");
      await page.keyboard.press("Escape");
      await expect(dialog).toBeHidden();

      await spots.nth(1).getByRole("link", { name: "Open the game at 27… Rf8" }).click();
      await page.waitForURL(/\/app\/games\/[^/]+\?ply=54$/);
      await expect(page.locator("main")).not.toContainText(/NaN|undefined|Infinity/);
    });
  });
