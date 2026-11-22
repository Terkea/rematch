import { expect, test } from "@playwright/test";
import { enterAge, FAKES, hydrated, signInByMagicLink } from "./support/mail";

// The cookie allow-list (M4 Task 28): after a full sign-in and linking flow, every cookie the
// browser holds is the app's own (its name starts `ci.` or `ci_`), and none is set for another
// domain. The app has no analytics, ads or third-party embeds, and this keeps it so.

// @fakes: linking needs the http chess.com fake, which a production server refuses.
test("after signing in and linking an account, every cookie is the app's own", {
  tag: "@fakes",
}, async ({ page, request, context, baseURL }) => {
  // The verify waits out chess.com's per-username interval (ADR-WEB-5).
  test.setTimeout(420_000);
  await signInByMagicLink(page, request, "cookies@example.invalid");
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 1990, 6);
  await expect(page).toHaveURL(/\/app\/accounts$/);
  await hydrated(page);

  // Link chess.com and verify it by location code.
  await page.getByLabel("chess.com username").fill("fakeplayer2");
  await page.getByRole("button", { name: "Connect chess.com" }).click();
  await expect(page.getByText("chess.com connected")).toBeVisible();
  const instructions = page.getByText(/to the Location field on your chess\.com profile/);
  const code = (await instructions.textContent())?.match(/Add (CI-[A-Z0-9]{6}) to/)?.[1];
  expect(code).toMatch(/^CI-[A-Z0-9]{6}$/);
  const set = await request.post(`${FAKES}/chesscom/__set-location`, {
    data: { username: "fakeplayer2", location: `York ${code}` },
  });
  expect(set.ok()).toBe(true);
  const check = page.getByRole("button", { name: "Check again" });
  await expect(check).toBeEnabled({ timeout: 90_000 });
  await check.click();
  await expect(page.getByText("chess.com account verified")).toBeVisible();

  // And the signed-in pages the flow leads to.
  await page.goto("/app");
  await page.waitForURL(/\/app/);

  const host = new URL(baseURL ?? "http://localhost:3100").hostname;
  const cookies = await context.cookies();
  expect(cookies.length, "the session is a cookie").toBeGreaterThan(0);
  for (const c of cookies) {
    expect(c.name, `cookie ${c.name} on ${c.domain}`).toMatch(/^ci[._]/);
    expect(c.domain.replace(/^\./, ""), `cookie ${c.name}'s domain`).toBe(host);
  }

  // Leave no verified link behind: the game-page seeds refuse a database that holds one.
  await page.goto("/app/accounts");
  await hydrated(page);
  await page.getByRole("button", { name: "Remove" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Remove account" }).click();
  await expect(
    page.getByText("Connect a Lichess or chess.com account to import your games."),
  ).toBeVisible();
});
