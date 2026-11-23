import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import type { E2eTrainItem } from "./helpers/seed-train";
import { adultPage, seedFixtures } from "./support/game-pages";
import { gotoSettled } from "./support/mail";

// The missed-moments trainer (ADR 0090), one session on the e2e-fixtures seed (synthetic): the
// player answers one position right and one wrong, each after saying how sure they are, sees the
// verdict and the engine's move, and finishes; every answered position is due again tomorrow.

function train(arg?: string): string {
  const run = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/helpers/seed-train.ts", ...(arg ? [arg] : [])],
    { env: process.env, encoding: "utf8" },
  );
  expect(run.status, run.stderr).toBe(0);
  return run.stdout;
}

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

/** Plays `uci` with the keyboard: focus the from square, Enter, the to square, Enter. */
async function play(page: Page, uci: string): Promise<void> {
  await page.locator(`[data-square-button="${uci.slice(0, 2)}"]`).focus();
  await page.keyboard.press("Enter");
  await page.locator(`[data-square-button="${uci.slice(2, 4)}"]`).focus();
  await page.keyboard.press("Enter");
}

async function shots(page: Page, name: string): Promise<void> {
  // Let the pen marks finish drawing.
  await page.waitForTimeout(1500);
  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    for (const width of [1440, 390]) {
      await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
      await page.evaluate(() => document.fonts.ready);
      const scroll = await page.evaluate(
        () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
      );
      expect(scroll, `no sideways scroll at ${width}`).toBe(false);
      await page.screenshot({
        path: `../../.shots/train/${name}-${width}-${scheme}.png`,
        fullPage: true,
      });
    }
  }
  await page.emulateMedia({ colorScheme: "light" });
  await page.setViewportSize({ width: 1440, height: 900 });
}

test("a training session: one right, one wrong, both due again tomorrow", async ({
  browser,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);
  const page = await adultPage(browser, request);
  seedFixtures();
  const items = JSON.parse(/TRAIN=(\[.*\])/.exec(train())?.[1] ?? "[]") as E2eTrainItem[];
  expect(items.length, "the seed gives at least two positions").toBeGreaterThanOrEqual(2);

  await settle(page, "/app/train");
  const main = page.locator("main");
  await expect(main.getByRole("heading", { level: 1, name: "Train" })).toBeVisible();
  await expect(main.getByRole("heading", { name: "Find the best move" })).toBeVisible();
  const first = await main.getByText(/^Position 1 of \d+/).textContent();
  const total = Number(/of (\d+)/.exec(first ?? "")?.[1]);
  expect(total).toBeGreaterThanOrEqual(2);
  await shots(page, "find");

  const answered: string[] = [];
  for (let i = 0; i < total; i++) {
    const id = await main.locator("section[data-item]").getAttribute("data-item");
    const item = items.find((x) => x.id === id);
    expect(item, `item ${id} is one of the seeded ones`).toBeTruthy();
    if (!item) return;
    const right = i !== 1;
    await play(page, right ? item.right : item.wrong);
    // The confidence comes before the verdict.
    await expect(main.locator('[data-slot="verdict"]')).toHaveCount(0);
    await expect(main.getByRole("group", { name: /How sure are you/ })).toBeVisible();
    if (i === 0) await shots(page, "confirm");
    await main.getByRole("button", { name: i === 1 ? "Sure" : "A guess" }).click();
    const verdict = main.locator('[data-slot="verdict"]');
    await expect(verdict).toContainText(
      right ? /^Right: / : /^Not this time: the engine's move was /,
    );
    const link = main.getByRole("link", { name: "Open the game at this move" });
    await expect(link).toHaveAttribute("href", /^\/app\/games\/[0-9a-f-]{36}\?ply=\d+$/);
    if (i === 1) await shots(page, "reveal-wrong");
    answered.push(item.id);
    await main
      .getByRole("button", { name: i + 1 === total ? "Finish session" : "Next position" })
      .click();
  }

  await expect(main.getByRole("heading", { name: "Today's session is done" })).toBeVisible({
    timeout: 60_000,
  });
  await expect(main).toContainText(`${total - 1} of ${total} positions right today.`);
  await expect(main.locator('[data-slot="due-tomorrow"]')).toHaveText(
    `${total} positions due tomorrow.`,
  );
  await expect(main.getByRole("heading", { name: "Is it working?" })).toBeVisible();
  await shots(page, "summary");

  const due = JSON.parse(/DUE=(\{.*\})/.exec(train("due"))?.[1] ?? "{}") as Record<string, string>;
  const tomorrow = new Date(Date.now() + 86_400_000).toISOString().slice(0, 10);
  for (const id of answered) expect(due[id]).toBe(tomorrow);
});
