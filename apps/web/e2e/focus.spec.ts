import { expect, type Page, test } from "@playwright/test";
import { adultPage, seedFixtures } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// The weekly focus (ADR 0091): "Make this my focus" on a weakness page sets it, the overview
// opens on its card (name, goal, games bar, rate against the target) with the practise call under
// it, and Clear
// focus takes it away. On the e2e-fixtures coach seed (synthetic): W3.shallow has 22 of 120
// chances.

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

test("setting a focus on a weakness page puts its card on the overview", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  await settle(page, "/app/weaknesses/W3.shallow");
  const name = (await page.locator("main h1").textContent())?.trim() ?? "";
  expect(name.length).toBeGreaterThan(0);

  const make = page.getByRole("button", { name: "Make this my focus" });
  // The button is a client island: retry the click until it has hydrated and the page refreshed.
  await expect(async () => {
    await make.click({ timeout: 2_000 });
    await expect(page.getByText("This is your focus this week.")).toBeVisible({ timeout: 5_000 });
  }).toPass({ timeout: 60_000 });

  await settle(page, "/app?tc=rapid");
  const card = page.getByRole("region", { name: "This week's focus" });
  await expect(card).toBeVisible();
  await expect(card.getByRole("link", { name })).toHaveAttribute(
    "href",
    /^\/app\/weaknesses\/W3\.shallow\?/,
  );
  await expect(card).toContainText(
    /Get it down to \d+\.\d% of your chances over your next 20 rapid games, from \d+\.\d% now\./,
  );
  await expect(
    card.getByRole("progressbar", { name: "Games played in this focus" }),
  ).toHaveAttribute("aria-valuemax", "20");
  await expect(card).toContainText("Too early to tell: 10 more chances");
  // The overview's one practise call (ADR 0092) follows the focus's source.
  await expect(card.getByRole("link", { name: "Practise this" })).toHaveCount(0);
  const practise = page.getByRole("region", { name: "Practise" });
  await expect(practise.getByRole("link").first()).toHaveAttribute(
    "href",
    /^\/app\/train\?account=[^&]+&tc=rapid(&source=W3\.shallow)?$/,
  );
  expect(await card.textContent()).not.toMatch(/NaN|undefined|Infinity/);

  // Shots for the critique loop (DESIGN.md "CL"): the repo's `.shots/` (git-ignored).
  for (const theme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: theme });
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
      await page.evaluate(() => document.fonts.ready);
      await card.screenshot({ path: `../../.shots/focus/card-${width}-${theme}.png` });
    }
  }
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 1440, height: 900 });

  // Progress shows the same focus.
  await settle(page, "/app/progress?tc=rapid");
  await expect(page.getByRole("region", { name: "This week's focus" })).toBeVisible();

  // Clearing it brings back a suggestion (or nothing), never the old focus.
  await settle(page, "/app?tc=rapid");
  await expect(async () => {
    await page
      .getByRole("region", { name: "This week's focus" })
      .getByRole("button", { name: "Clear focus" })
      .click({ timeout: 2_000 });
    await expect(page.getByRole("region", { name: "This week's focus" })).toHaveCount(0, {
      timeout: 5_000,
    });
  }).toPass({ timeout: 60_000 });
  const offer = page.getByRole("region", { name: "A focus for this week" });
  if ((await offer.count()) > 0) {
    await expect(offer.getByRole("button", { name: "Make this my focus" })).toBeVisible();
    await offer.screenshot({ path: "../../.shots/focus/suggest-1440-light.png" });
  }
  await page.context().close();
});
