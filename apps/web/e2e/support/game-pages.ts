import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";
import { type APIRequestContext, type Browser, expect, type Page } from "@playwright/test";
// Type only: the seed module loads @ci/db, which only tsx (with the development condition) resolves.
import type { E2eGames } from "../helpers/seed-games";
import { enterAge, gotoSettled, signInByMagicLink } from "./mail";

// Shared by the game-page specs (M5a Task 13): the adult e2e user's page, and the seeded
// e2e-fixtures games (e2e/helpers/seed-games.ts, synthetic and public fixture data only).

/** The seed's user (E2E_USER in e2e/helpers/seed-games.ts). */
export const E2E_USER = "adult@example.invalid";
/** Saved by accounts.spec.ts. */
const ADULT_STATE = "e2e/.auth/adult.json";

/** The adult's page: the stored state when it still works, else a fresh magic-link sign-in. */
export async function adultPage(browser: Browser, request: APIRequestContext): Promise<Page> {
  const context = await browser.newContext(
    existsSync(ADULT_STATE) ? { storageState: ADULT_STATE } : {},
  );
  const page = await context.newPage();
  await gotoSettled(page, "/");
  await page.waitForURL((url) => url.pathname !== "/", { timeout: 120_000 });
  if (new URL(page.url()).pathname.startsWith("/sign-in")) {
    await signInByMagicLink(page, request, E2E_USER);
    if (new URL(page.url()).pathname === "/welcome/age") {
      await enterAge(page, 1990, 6);
      // Let the age screen's own navigation land before the caller navigates.
      await page.waitForURL(/\/app\/accounts$/, { timeout: 120_000 });
    }
    // Later specs and projects in the run reuse the session (the sign-in rate limit is per email).
    await mkdir(dirname(ADULT_STATE), { recursive: true });
    await context.storageState({ path: ADULT_STATE });
  }
  return page;
}

export function seedFixtures(): E2eGames {
  const run = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/helpers/seed-games.ts", E2E_USER],
    { env: process.env, encoding: "utf8" },
  );
  expect(run.status, run.stderr).toBe(0);
  const line = /SEED=(\{.*\})/.exec(run.stdout)?.[1];
  expect(line, run.stdout).toBeTruthy();
  return JSON.parse(line as string) as E2eGames;
}
