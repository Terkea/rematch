import { spawnSync } from "node:child_process";
import { FAKES, MAILPIT } from "./support/mail";

const BASE = process.env.E2E_BASE_URL ?? "http://localhost:3100";

async function waitFor(url: string, ms: number): Promise<void> {
  const deadline = Date.now() + ms;
  for (;;) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // not up yet
    }
    if (Date.now() > deadline)
      throw new Error(`globalSetup: ${url} did not answer within ${ms} ms`);
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

/**
 * Every run starts from nothing (review of Task 21): the e2e database is created if missing,
 * migrated and emptied; Mailpit is emptied; the provider fakes are reset. Then the web app must
 * answer its health check, which needs that database.
 */
export default async function globalSetup(): Promise<void> {
  const reset = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/support/reset-db.ts"],
    {
      stdio: "inherit",
      env: process.env,
    },
  );
  if (reset.status !== 0) throw new Error("globalSetup: resetting the e2e database failed");

  const mail = await fetch(`${MAILPIT}/api/v1/messages`, { method: "DELETE" });
  if (!mail.ok) throw new Error(`globalSetup: clearing Mailpit gave ${mail.status}`);
  const fakes = await fetch(`${FAKES}/chesscom/__reset`, { method: "POST" });
  if (!fakes.ok) throw new Error(`globalSetup: resetting the fakes gave ${fakes.status}`);

  await waitFor(`${BASE}/api/health`, 180_000);
}
