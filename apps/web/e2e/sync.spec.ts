import { expect, type Page, test } from "@playwright/test";
import { seedCoach, simulate } from "./support/coach";
import { adultPage } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// Sync now and the analysis-progress panel (M5b Task P2, ADR 0077) on the e2e-fixtures account.
// No worker runs: the click enqueues the real `import-account` job, and `simulate` plays the
// import finishing (its job, its run, `import.progress {done: true}`), as the Rust import does.

async function live(page: Page, url: string): Promise<void> {
  const opened = page.waitForResponse((r) => new URL(r.url()).pathname === "/api/events", {
    timeout: 180_000,
  });
  await gotoSettled(page, url);
  await page.waitForFunction(
    () => {
      const el = document.querySelector("[data-slot='sync-panel'] button");
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    undefined,
    { timeout: 180_000 },
  );
  await opened;
}

test("Sync now checks for new games, then reports the result and its cooldown", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs in the e2e project");
  test.setTimeout(600_000);
  const page = await adultPage(browser, request);
  const { accountId } = seedCoach();
  try {
    await live(page, "/app/games");
    const panel = page.locator("[data-slot='sync-panel']");
    await expect(panel.getByText("Not synced yet.").first()).toBeVisible();
    // Every count is stated: the seed's one pending game is waiting.
    await expect(panel.getByText(/of \d+ games analysed/).first()).toBeVisible();

    await panel
      .getByRole("button", { name: /^Sync now/ })
      .first()
      .click();
    await expect(panel.getByText(/Checking for new games/).first()).toBeVisible();
    // While it checks, Sync now stays focusable but is unavailable, and says why.
    await expect(panel.getByRole("button", { name: /^Sync now/ }).first()).toHaveAttribute(
      "aria-disabled",
      "true",
    );

    simulate({ kind: "import-done", accountId, imported: 0 });
    await expect(panel.getByText("Synced just now. No new games.").first()).toBeVisible();
    await expect(panel.getByText(/You can sync again in \d min\./).first()).toBeAttached();

    // The overview's compact panel reads the same state.
    await live(page, "/app");
    const compact = page.locator("[data-slot='sync-panel'][data-variant='compact']");
    await expect(compact.getByText(/Synced just now/)).toBeVisible();
    await expect(compact.getByRole("link", { name: "Details" })).toHaveAttribute(
      "href",
      "/app/games",
    );
  } finally {
    seedCoach();
    await page.context().close();
  }
});
