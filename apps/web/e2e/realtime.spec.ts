import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import { enterAge, gotoSettled, signInByMagicLink } from "./support/mail";

// The signed-in shell's live updates (M4 Task 27): the layout mounts the one event stream and
// the header's activity indicator. An event stored for the user's linked (synthetic Lichess)
// account reaches the header over SSE; with SSE blocked, the 5 s polling fallback
// (`GET /api/events?after=`) still brings the next one (ADR-WEB-8).

const EMAIL = "realtime@example.invalid";

function publish(imported: number, total: number): void {
  const run = spawnSync(
    "node_modules/.bin/tsx",
    [
      "--conditions",
      "development",
      "e2e/support/publish-event.ts",
      EMAIL,
      `${imported}`,
      `${total}`,
    ],
    { stdio: "inherit", env: process.env },
  );
  expect(run.status).toBe(0);
}

const activity = (page: Page) => page.getByRole("banner").locator('[data-slot="activity"]');

test("import progress reaches the header over SSE, and by polling when SSE is blocked", async ({
  page,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(300_000);

  await signInByMagicLink(page, request, EMAIL);
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 1990, 6);
  await expect(page).toHaveURL(/\/app\/accounts$/);

  // Link the account (its first event predates the page, so it is not replayed), then load a
  // fresh page, whose stream opens with the account in scope.
  publish(0, 400);
  const opened = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === "/api/events" && !new URL(r.url()).searchParams.has("after"),
  );
  await gotoSettled(page, "/app/accounts");
  await opened;

  publish(120, 400);
  await expect(activity(page)).toContainText("Importing 120 / 400 games", { timeout: 2_000 });

  // No SSE: every stream request fails, so the client falls back to polling `?after=`.
  await page.route(
    (url) => url.pathname === "/api/events" && !url.searchParams.has("after"),
    (route) => route.abort(),
  );
  // The first poll's answer, not just its request: under next dev it compiles the route first.
  const polled = page.waitForResponse((r) => new URL(r.url()).searchParams.has("after"));
  await gotoSettled(page, "/app/accounts");
  await polled;
  publish(200, 400);
  await expect(activity(page)).toContainText("Importing 200 / 400 games", { timeout: 7_000 });
});
