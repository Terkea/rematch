import { expect, type Page, test } from "@playwright/test";
import { adultPage, seedFixtures } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// The weakness pages explain themselves (feat/explain): the detail page reads top down as a story
// (what it is, how often, better or worse, the player's own examples, what to do, then the
// details), and a list row's "i" opens the glossary entry on click, closing on Escape with focus
// back on the trigger. On the e2e-fixtures coach seed (synthetic): W3.shallow has 22 of 120
// chances and three stored example blunders in the newest rapid games.

async function settle(page: Page, url: string): Promise<void> {
  await gotoSettled(page, url);
  await page.waitForFunction(
    () => {
      const el = document.querySelector("main h1");
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    undefined,
    { timeout: 180_000 },
  );
}

test("the weakness detail tells its story, with the player's own examples", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  await settle(page, "/app/weaknesses/W3.shallow");

  const heads = await page.locator("main h2").allTextContents();
  expect(heads.slice(0, 6)).toEqual([
    "What this is",
    "How often it happens to you",
    "Better or worse",
    "Examples from your games",
    "What to do",
    "Details",
  ]);
  const main = page.locator("main");
  await expect(main).toContainText(
    "You had a quick tactic that won material or mated, and you played something else.",
  );
  await expect(main).toContainText(
    "In the last 6 months this happened in 22 of your 120 chances (about 1 in 5).",
  );
  await expect(main).toContainText("80% range");

  const examples = page.locator("section:has(#examples-heading) > ol > li");
  await expect(examples).toHaveCount(3);
  const first = examples.first();
  // The played move (pen) and the engine's move (good) as two arrows, named in the key.
  await expect(first.getByRole("list", { name: "Arrows" }).locator("li")).toHaveCount(2);
  await expect(first).toContainText(
    "Your win chance went from 62% to 31%, a drop of 31 win% points.",
  );
  const link = first.getByRole("link", { name: /in the review/ });
  await expect(link).toHaveAttribute("href", /^\/app\/games\/[0-9a-f-]{36}\?ply=\d+$/);

  // The charts are kept, under Details.
  await expect(page.getByRole("heading", { name: "State over time" })).toBeVisible();
  expect(await main.textContent()).not.toMatch(/NaN|undefined|Infinity/);

  await link.click();
  await page.waitForURL(/\/app\/games\/[0-9a-f-]{36}\?ply=\d+$/);
  await page.context().close();
});

test("a weakness row's explanation opens on click and closes on Escape", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  await settle(page, "/app/weaknesses");

  const trigger = page.getByRole("button", { name: "What is missing a short tactic?" });
  await expect(trigger).toHaveAttribute("aria-expanded", "false");
  await trigger.hover();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await trigger.click();
  // Opening the explanation never follows the row's link.
  await expect(page).toHaveURL(/\/app\/weaknesses(\?|$)/);
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("How it's measured");
  await expect(dialog).toContainText("What to try");
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await page.context().close();
});

test("each opening verdict on the report says why, with its rule a click away", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  await gotoSettled(page, "/app/report");
  await page.locator("[data-report-version]").waitFor({ timeout: 180_000 });

  const openings = page.getByRole("region", { name: "Openings" });
  await expect(openings).toContainText(
    "Fix means one move keeps costing you: Qc7, in 7 of your 23 games in this line, losing 10 win% points or more each time.",
  );
  await expect(openings).toContainText(
    "Keep means no single move keeps costing you, and your +4 pp against what your rating predicts is within normal variation for 41 rated games (likely −3 to +11 pp).",
  );
  await expect(openings).toContainText(
    "Learn deeper means you leave the opening worse off: your win chance averages 38% when it ends, over 9 analysed games.",
  );
  const trigger = openings.getByRole("button", { name: "What does Replace mean?" });
  await expect(trigger).toHaveCount(0);
  const fix = openings.getByRole("button", { name: "What does Fix mean?" });
  // Hydrated before the click, or the popover has no handler yet.
  await page.waitForFunction(
    () => {
      const el = document.querySelector('[aria-label="What does Fix mean?"]');
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    undefined,
    { timeout: 180_000 },
  );
  await fix.click();
  await expect(page.getByRole("dialog")).toContainText("How it's measured");
  await page.keyboard.press("Escape");
  await expect(fix).toBeFocused();
  expect(await openings.textContent()).not.toMatch(/NaN|undefined|Infinity/);
  await page.context().close();
});
