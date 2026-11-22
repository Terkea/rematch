import { type APIRequestContext, expect, type Page } from "@playwright/test";

// Shared by the e2e specs: request a magic link on /sign-in and fetch it from Mailpit.
// Runs against the compose stack with docker-compose.e2e.yml (which invites these addresses).
export const MAILPIT = process.env.MAILPIT_URL ?? "http://localhost:48025";
export const FAKES = process.env.FAKES_URL ?? "http://localhost:53199";

type MailpitSearch = { messages: { ID: string; Created: string }[] };

/** The newest magic link mailed to `email` since `since` (ms). */
export async function magicLinkFor(
  request: APIRequestContext,
  email: string,
  since: number,
): Promise<string> {
  let link: string | undefined;
  await expect
    .poll(
      async () => {
        const search = (await (
          await request.get(`${MAILPIT}/api/v1/search?query=${encodeURIComponent(`to:${email}`)}`)
        ).json()) as MailpitSearch;
        const newest = search.messages
          .filter((m) => Date.parse(m.Created) >= since - 1000)
          .sort((a, b) => Date.parse(b.Created) - Date.parse(a.Created))[0];
        if (!newest) return undefined;
        const message = (await (
          await request.get(`${MAILPIT}/api/v1/message/${newest.ID}`)
        ).json()) as { Text: string };
        link = message.Text.match(/https?:\/\/\S+\/api\/auth\/magic-link\/verify\S+/)?.[0];
        return link;
      },
      { timeout: 15_000 },
    )
    .toBeTruthy();
  return link as string;
}

/** Signs in through the real form and opens the mailed link. */
export async function signInByMagicLink(
  page: Page,
  request: APIRequestContext,
  email: string,
): Promise<void> {
  const since = Date.now();
  await page.goto("/sign-in", { waitUntil: "networkidle" });
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Email me a sign-in link" }).click();
  await expect(page.getByRole("heading", { name: "Check your email" })).toBeVisible();
  await expect(
    page.getByText(`We sent a sign-in link to ${email}. It expires in 10 minutes.`),
  ).toBeVisible();
  const link = await magicLinkFor(request, email, since);
  // `next dev` may compile the verify route and its redirect target on first use, so wait for
  // the redirect chain to land rather than for one load event.
  await page.goto(link, { waitUntil: "commit", timeout: 90_000 });
  await page.waitForURL((url) => !url.pathname.startsWith("/api/"), { timeout: 90_000 });
}

/** Answers the age screen with a birth month. */
export async function enterAge(page: Page, year: number, month: number): Promise<void> {
  await expect(page.getByRole("heading", { name: "When were you born?" })).toBeVisible();
  // Hydrated first, so the press runs the form's handler (a dev server hydrates slowly).
  await page.waitForLoadState("networkidle");
  await page.getByLabel("Month").selectOption(String(month));
  await page.getByLabel("Year").selectOption(String(year));
  await page.getByRole("button", { name: "Continue" }).click();
}
