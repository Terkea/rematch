import { expect, type Page, test } from "@playwright/test";
import { latestRun, seedCoach, simulate } from "./support/coach";
import { adultPage } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// The report page's own flows (M5b Task 6, Task 11): "Build my report" from no report, the five
// sections arriving one by one through the simulated worker (no AI, ADR 0074), and the failure
// throttle. The seed has no report for the scope; the last step restores the full coach world.

const SECTIONS = ["overview", "weaknesses", "areas", "openings", "climb_plan"] as const;

async function live(page: Page, url: string): Promise<void> {
  const opened = page.waitForResponse((r) => new URL(r.url()).pathname === "/api/events", {
    timeout: 180_000,
  });
  await gotoSettled(page, url);
  await page.waitForFunction(
    () => {
      const el = document.querySelector("main button");
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    undefined,
    { timeout: 180_000 },
  );
  await opened;
}

test.describe
  .serial("the report page", () => {
    let page: Page;

    test.beforeAll(async ({ browser, request }, info) => {
      test.skip(info.project.name !== "e2e", "runs in the e2e project");
      test.setTimeout(600_000);
      page = await adultPage(browser, request);
    });

    test.afterAll(async () => {
      seedCoach();
      await page?.context().close();
    });

    test("Build my report: the sections arrive in order, without a reload", async () => {
      test.setTimeout(600_000);
      seedCoach({ report: false });
      await live(page, "/app/report");
      await expect(
        page.getByText("Your coaching report hasn't been built yet.").first(),
      ).toBeVisible();
      await page.getByRole("button", { name: "Build my report" }).click();
      await expect(
        page.getByText("Building your report. Sections appear as they're written."),
      ).toBeVisible();
      await expect(page.getByText(/Building your report: 0 of 5 sections written/)).toBeVisible();
      const runId = latestRun();

      // A marker on the window survives client-side refreshes, not a reload.
      await page.evaluate(() => {
        (window as unknown as { __noReload: boolean }).__noReload = true;
      });
      for (const [i, section] of SECTIONS.slice(0, 4).entries()) {
        simulate({ kind: "report-section", runId, section });
        await expect(
          page.getByText(new RegExp(`Building your report: ${i + 1} of 5 sections written`)),
        ).toBeVisible();
      }
      simulate({ kind: "report-section", runId, section: "climb_plan" });
      await expect(page.getByText(/sections written/)).toHaveCount(0);
      await expect(page.getByRole("heading", { name: "Your climb plan" })).toBeVisible();
      await expect(page.getByRole("heading", { name: "What's costing you games" })).toBeVisible();
      await expect(page.getByText("Written by AI").first()).toBeVisible();
      expect(
        await page.evaluate(() => (window as unknown as { __noReload?: boolean }).__noReload),
        "no reload",
      ).toBe(true);
    });

    test("a failed build says so; trying again at once is throttled with its time", async () => {
      test.setTimeout(600_000);
      seedCoach({ report: false });
      await live(page, "/app/report");
      await page.getByRole("button", { name: "Build my report" }).click();
      await expect(page.getByText(/Building your report: 0 of 5 sections written/)).toBeVisible();
      simulate({ kind: "report-failed", runId: latestRun(), reason: "ai_unavailable" });
      await expect(page.getByText("Building your report failed. Try again.").first()).toBeVisible();
      await page.getByRole("button", { name: "Try again" }).click();
      await expect(
        page.getByText(
          /^Your last report attempts didn't finish, so rebuilding is paused\. Try again .+\.$/,
        ),
      ).toBeVisible();
    });
  });
