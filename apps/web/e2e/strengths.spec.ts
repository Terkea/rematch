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
      const w8 = improving.locator('[data-claim="W8.basics"]');
      await expect(w8.locator("[data-claim-figure]")).toHaveText(
        "9.0% of chances now, down from 14.0%",
      );
      await expect(w8.locator("[data-claim-state]")).toHaveText("Improving");
      await expect(w8).toContainText("Detected on 15 Oct 2026, 85% likely a real change.");
      // Sections with nothing to claim are one quiet list at the bottom (critic 9).
      await expect(page.locator('[data-section="not-judged"] [data-section="above"]')).toHaveCount(
        1,
      );
      await expect(page.locator('[data-section="openings"]')).toContainText("Not enough games yet");
      await expect(page.locator("[data-report-strengths]")).toContainText(
        "Solid Italian repertoire",
      );
      await expect(page.locator("main")).not.toContainText(/NaN|undefined|Infinity/);
    });

    // Audit A5: the shared filter bar, each colour in Both mode, a "What is this?" on each claim.
    test("filters by colour and dates, splits Both by colour, and explains each claim", async () => {
      test.setTimeout(300_000);
      await gotoSettled(page, "/app/strengths");
      const byColour = page.locator('[data-section="by-colour"]');
      await expect(byColour.locator("[data-colour]")).toHaveCount(2);
      // The strengths seed has no benchmark: each colour says so instead of a claim.
      await expect(byColour.locator("[data-colour='white']")).toContainText(
        "No benchmark for your White games yet.",
      );
      const claim = page.locator('[data-section="improving"] [data-claim="W8.basics"]');
      await expect(claim).toContainText("Basic endgames");
      await expect(async () => {
        await claim.getByRole("button", { name: "What is basic endgames?" }).click();
        await expect(page.getByRole("dialog", { name: "Basic endgames" })).toBeVisible({
          timeout: 2_000,
        });
      }).toPass({ timeout: 60_000 });
      await page.keyboard.press("Escape");

      const bar = page.locator("[data-filter-bar] > div");
      await bar.getByRole("group", { name: "Colour" }).getByRole("link", { name: "Black" }).click();
      await expect(page).toHaveURL(/\/app\/strengths\?.*color=black/);
      await expect(
        page.getByText(/^Based on \d+ analysed rapid games? with Black on chess\.com\./),
      ).toBeVisible();
      await expect(page.locator('[data-section="by-colour"]')).toHaveCount(0);
      await expect(page.locator('[data-section="improving"]')).toContainText(
        "Weakness changes are called on all your games, both colours.",
      );

      await gotoSettled(page, "/app/strengths?from=2026-10-01&to=2026-10-31");
      await expect(page.locator("[data-slot='dates-note']")).toContainText(
        "These dates narrow the comparison with your earlier games",
      );
      // The page's filters follow the header to the next page (audit A7).
      await page
        .getByRole("navigation", { name: "Main" })
        .getByRole("link", { name: "Weaknesses" })
        .click();
      await expect(page).toHaveURL(/\/app\/weaknesses\?.*from=2026-10-01&to=2026-10-31/);
      await expect(page.locator("[data-slot='dates-note']")).toContainText(
        "these dates trim the weekly lines only",
      );
      await expect(page.locator("main")).not.toContainText(/NaN|undefined|Infinity/);

      // Shots for the critique loop, signed in: the repo's `.shots/` (git-ignored).
      await page.addInitScript(() => {
        try {
          localStorage.removeItem("theme");
        } catch {
          // storage unavailable: the colour-scheme emulation still applies
        }
      });
      const routes = [
        ["both", "/app/strengths"],
        ["black-dated", "/app/strengths?color=black&from=2026-10-01&to=2026-10-31"],
      ] as const;
      for (const width of [1440, 390]) {
        await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
        for (const scheme of ["light", "dark"] as const) {
          await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
          for (const [name, url] of routes) {
            await gotoSettled(page, url);
            await page.evaluate(() => document.fonts.ready);
            expect(
              await page.evaluate(
                () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
              ),
              `${name} at ${width}`,
            ).toBeLessThanOrEqual(0);
            await page.screenshot({
              path: `../../.shots/strengths/${name}/${width}-${scheme}.png`,
              fullPage: true,
            });
          }
        }
      }
      await page.setViewportSize({ width: 1440, height: 900 });
      await page.emulateMedia({ colorScheme: "light", reducedMotion: "reduce" });
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
      await expect(hard).toContainText("Until it does, these are moments where the engine");
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
