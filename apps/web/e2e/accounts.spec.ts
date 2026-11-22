import { mkdir } from "node:fs/promises";
import { expect, test } from "@playwright/test";
import { enterAge, FAKES, gotoSettled, hydrated, signInByMagicLink } from "./support/mail";

// Runs against the compose stack with the fakes:
//   docker compose -f docker-compose.yml -f docker-compose.e2e.yml --profile e2e up -d
// The adult flow saves its signed-in storage state for `pnpm shots --storage`.
const ADULT_STATE = "e2e/.auth/adult.json";

test("an adult links chess.com, verifies it by location code, then removes it", async ({
  page,
  request,
  context,
}) => {
  // The first verify waits out chess.com's 60 s per-username interval (ADR-WEB-5).
  test.setTimeout(420_000);

  await signInByMagicLink(page, request, "adult@example.invalid");
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 1990, 6);
  await expect(page).toHaveURL(/\/app\/accounts$/);
  await hydrated(page);
  await mkdir("e2e/.auth", { recursive: true });
  await context.storageState({ path: ADULT_STATE });

  // The shell.
  const header = page.getByRole("banner");
  await expect(header.getByRole("link", { name: "Accounts" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(header.locator('[data-slot="activity"]')).toHaveCount(1);

  // Empty state.
  await expect(page.getByRole("heading", { name: "Accounts", level: 1 })).toBeVisible();
  await expect(
    page.getByText("Connect a Lichess or chess.com account to import your games."),
  ).toBeVisible();

  // Connect chess.com: a pending row with a code.
  await page.getByLabel("chess.com username").fill("fakeplayer");
  await page.getByRole("button", { name: "Connect chess.com" }).click();
  await expect(page.getByText("chess.com connected")).toBeVisible();
  const instructions = page.getByText(/to the Location field on your chess\.com profile/);
  await expect(instructions).toContainText(
    "then check again. chess.com can take up to 12 hours to show profile changes.",
  );
  const code = (await instructions.textContent())?.match(/Add (CI-[A-Z0-9]{6}) to/)?.[1];
  expect(code).toMatch(/^CI-[A-Z0-9]{6}$/);
  await expect(page.getByRole("button", { name: "Copy code" })).toBeVisible();

  // The player sets the location; "Check again" opens once the interval has passed.
  const set = await request.post(`${FAKES}/chesscom/__set-location`, {
    data: { username: "fakeplayer", location: `Leeds ${code}` },
  });
  expect(set.ok()).toBe(true);
  const check = page.getByRole("button", { name: "Check again" });
  await expect(check).toBeEnabled({ timeout: 90_000 });
  await check.click();
  await expect(page.getByText("chess.com account verified")).toBeVisible();
  await expect(page.getByText("Verified", { exact: true })).toBeVisible();

  // Remove it.
  await page.getByRole("button", { name: "Remove" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("Remove FakePlayer?")).toBeVisible();
  await expect(
    dialog.getByText("Your imported games stay, but they won't sync to your account."),
  ).toBeVisible();
  await dialog.getByRole("button", { name: "Remove account" }).click();
  await expect(
    page.getByText("Connect a Lichess or chess.com account to import your games."),
  ).toBeVisible();
});

// Lichess accounts are deferred (ADR 0040); Task 18 brings the provider and its fake.
test("Continue with Lichess signs a new player in through the fake", async ({ browser }) => {
  test.fixme(process.env.E2E_LICHESS !== "1", "Lichess accounts are deferred (ADR 0040, Task 18)");
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto("/sign-in");
  await page.getByRole("button", { name: "Continue with Lichess" }).click();
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await context.close();
});

test("an under-13 is blocked and the sign-in page stays blocked", async ({ page, request }) => {
  await signInByMagicLink(page, request, "kid@example.invalid");
  await expect(page).toHaveURL(/\/welcome\/age$/);
  // The page guard: no app page opens before the age is answered (review I2).
  await gotoSettled(page, "/app/accounts");
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 2015, 1);
  await expect(page).toHaveURL(/\/blocked$/);
  await expect(page.getByText("You need to be 13 or older to use chess-improver.")).toBeVisible();
  await expect(page.getByText("We've deleted the details you gave us.")).toBeVisible();

  await gotoSettled(page, "/sign-in");
  await expect(page.getByText("You need to be 13 or older to use chess-improver.")).toBeVisible();
  await expect(page.getByLabel("Email")).toHaveCount(0);
});
