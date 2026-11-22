import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import { enterAge, gotoSettled, hydrated, signInByMagicLink } from "./support/mail";

// The analysis reveal on a phone (M5a T11, fix round 1): nothing moves around the board while it
// plays (I1, I2), and a press on Skip never becomes a Replay (review m4). Synthetic data only
// (e2e/support/seed-review.ts).

const EMAIL = "reveal@example.invalid";

declare global {
  interface Window {
    __cls: number;
  }
}

const phase = (page: Page) => page.getAttribute("[data-review]", "data-reveal");
const boardTop = (page: Page) =>
  page.evaluate(() => document.querySelector("[data-board]")?.getBoundingClientRect().top ?? -1);

test("the reveal holds the layout still at 390 px, and Skip presses never replay", async ({
  page,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(420_000);

  await signInByMagicLink(page, request, EMAIL);
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 1990, 6);
  await expect(page).toHaveURL(/\/app\/accounts$/);

  const seed = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/support/seed-review.ts", EMAIL],
    { env: process.env, encoding: "utf8" },
  );
  expect(seed.status, seed.stderr).toBe(0);
  const gameId = /GAME=([0-9a-f-]{36})/.exec(seed.stdout)?.[1];
  expect(gameId).toBeTruthy();

  await page.setViewportSize({ width: 390, height: 844 });
  // Compile the route without spending the first view: a ?ply= never runs the reveal.
  await gotoSettled(page, `/app/games/${gameId}?ply=1`);
  await hydrated(page, "[data-review] [data-reveal-controls='phone']");

  // Layout shifts without recent input, from the first paint of the real first view.
  await page.addInitScript(() => {
    window.__cls = 0;
    new PerformanceObserver((list) => {
      for (const e of list.getEntries() as (PerformanceEntry & {
        value: number;
        hadRecentInput: boolean;
      })[])
        if (!e.hadRecentInput) window.__cls += e.value;
    }).observe({ type: "layout-shift", buffered: true });
  });
  await gotoSettled(page, `/app/games/${gameId}`);
  await expect
    .poll(() => page.getAttribute("[data-review]", "data-reveal-mode"), { timeout: 180_000 })
    .toBe("full");
  const top = await boardTop(page);
  await expect.poll(() => phase(page), { timeout: 20_000 }).toBe("done");
  expect(await boardTop(page)).toBe(top);
  expect(await page.evaluate(() => window.__cls)).toBeLessThan(0.02);

  // A pointer press on Skip ends the run and does not click the Replay that takes its place.
  const replay = page.getByRole("button", { name: "Replay analysis" });
  const skip = page.getByRole("button", { name: "Skip" });
  for (const how of ["click", "Enter", "Space"] as const) {
    await replay.click();
    await expect.poll(() => phase(page)).toBe("running");
    if (how === "click") await skip.click();
    else {
      await skip.focus();
      await page.keyboard.press(how);
    }
    await expect.poll(() => phase(page)).toBe("done");
    await page.waitForTimeout(500);
    expect(await phase(page), `after ${how} on Skip`).toBe("done");
  }
});
