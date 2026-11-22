import { expect, test } from "@playwright/test";
import { signInByMagicLink } from "./support/mail";

// Runs against the compose stack with Mailpit (docker-compose.e2e.yml puts this address on the
// invite allow-list).
test("signs in with a magic link from Mailpit and lands on the age screen", async ({
  page,
  request,
}) => {
  await signInByMagicLink(page, request, "dev@example.invalid");
  await expect(page).toHaveURL(/\/welcome\/age$/);
});

test("a used magic link shows the expired message", async ({ page }) => {
  await page.goto("/sign-in?error=link_expired");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  await expect(page.getByText("This link has expired. Request a new one.")).toBeVisible();
});
