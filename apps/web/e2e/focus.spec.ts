import { spawnSync } from "node:child_process";
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
  // The whole overview (ADR 0092): one tall viewport per capture, once the trend has drawn
  // ("Tried and rejected": a full-page capture caught it mid-measure).
  for (const theme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: theme, reducedMotion: "reduce" });
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
      await page.screenshot({ path: `../../.shots/overview/first-${width}-${theme}.png` });
      const height = await page.evaluate(() => document.documentElement.scrollHeight);
      await page.setViewportSize({ width, height });
      await page.evaluate(() => document.fonts.ready);
      await expect(page.locator("[data-chart='trend'] svg[role='img']")).toBeVisible();
      await page.waitForTimeout(500);
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        ),
      ).toBeLessThanOrEqual(0);
      await page.screenshot({ path: `../../.shots/overview/${width}-${theme}.png` });
    }
  }
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 1440, height: 900 });

  // Change N: the running focus keeps its start, now over 10 games.
  await expect(async () => {
    await card.getByRole("combobox", { name: "Length" }).click({ timeout: 2_000 });
    await page.getByRole("option", { name: "10 games" }).click({ timeout: 2_000 });
    await expect(
      card.getByRole("progressbar", { name: "Games played in this focus" }),
    ).toHaveAttribute("aria-valuemax", "10", { timeout: 5_000 });
  }).toPass({ timeout: 60_000 });
  await expect(card).toContainText("over your next 10 rapid games");

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

function focusSeed(state: string): void {
  const run = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/helpers/seed-focus.ts", state],
    { env: process.env, encoding: "utf8" },
  );
  expect(run.status, run.stderr).toBe(0);
}

test("a focus that reached its N games reads as closed: met, not met, inconclusive", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  const outcomes = [
    ["met", "Goal met"],
    ["not_met", "Goal not met"],
    ["inconclusive", null],
  ] as const;
  for (const [kind, text] of outcomes) {
    focusSeed(kind);
    await settle(page, "/app?tc=rapid");
    const card = page.getByRole("region", { name: "Your last focus" });
    await expect(card).toBeVisible();
    await expect(card.locator(`[data-verdict="${kind}"]`)).toBeVisible();
    if (text) await expect(card.locator(`[data-verdict="${kind}"]`)).toHaveText(text);
    await expect(card).toContainText("Over 5 games");
    expect(await card.textContent()).not.toMatch(/NaN|undefined|Infinity/);
    // Progress reads the same closed focus.
    await settle(page, "/app/progress?tc=rapid");
    await expect(page.getByRole("region", { name: "Your last focus" })).toBeVisible();
  }
  focusSeed("clear");
  await page.context().close();
});

test("a weakness whose rate cannot go down has no target: 422 no_target", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  focusSeed("zero");
  await settle(page, "/app?tc=rapid");
  // The overview's trainer link names the rapid scope's account.
  const href = await page.locator('[data-slot="practise"] a').first().getAttribute("href");
  const accountId = new URL(href ?? "", "http://x").searchParams.get("account");
  expect(accountId).toMatch(/^[0-9a-f-]{36}$/);
  const res = await page.request.post("/api/focus", {
    data: { accountId, timeClass: "rapid", seriesKey: "W2" },
    headers: { origin: new URL(page.url()).origin },
  });
  expect(res.status()).toBe(422);
  expect(await res.json()).toEqual({ error: "no_target" });
  focusSeed("clear");
  await page.context().close();
});
