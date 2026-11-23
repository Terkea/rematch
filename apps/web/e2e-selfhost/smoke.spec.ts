import { expect, type Page, test } from "@playwright/test";
import { enterAge, gotoSettled, signInByMagicLink } from "../e2e/support/mail";

// The self-host smoke (ADR 0097): what a stranger gets from `sh scripts/setup.sh minimal
// --ai=none` and `pnpm seed:demo`. Sign in through Mailpit, see the demo player on the overview,
// open the trainer and the computed coach report. It runs against the dev database, so it only
// reads; signing in twice is fine (the age screen shows once).

const EMAIL = process.env.SMOKE_EMAIL ?? "player@rematch.test";

async function settle(page: Page, url: string): Promise<void> {
  await gotoSettled(page, url);
  await page.waitForFunction(
    () => {
      const el = document.querySelector("main h1");
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    undefined,
    { timeout: 300_000 },
  );
}

test("sign in, the demo overview, the trainer and the report", async ({ page, request }) => {
  await signInByMagicLink(page, request, EMAIL);
  if (await page.getByRole("heading", { name: "When were you born?" }).isVisible()) {
    await enterAge(page, 1990, 6);
  }

  await settle(page, "/app");
  await expect(page.getByText("Demo data", { exact: true })).toBeVisible();
  await expect(page.getByRole("region", { name: "Practise" })).toBeVisible();
  expect(await page.locator("main").textContent()).not.toMatch(/NaN|undefined|Infinity/);

  await settle(page, "/app/train");
  const main = page.locator("main");
  await expect(main.getByRole("heading", { level: 1, name: "Train" })).toBeVisible();
  await expect(main.getByRole("heading", { name: "Find the best move" })).toBeVisible();

  await settle(page, "/app/report");
  await expect(page.getByRole("heading", { name: "Your climb plan" })).toBeVisible();
});
