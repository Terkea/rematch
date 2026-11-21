import { expect, test } from "@playwright/test";

// Runs against the compose stack with Mailpit (docker-compose.e2e.yml puts this address on the
// invite allow-list). Needs /sign-in and /welcome/age from Task 21, so it stays fixme until then.
const MAILPIT = process.env.MAILPIT_URL ?? "http://localhost:48025";
const email = "dev@example.invalid";

type MailpitSearch = { messages: { ID: string; Created: string }[] };

test.fixme("signs in with a magic link from Mailpit and lands on the age screen", async ({
  page,
  request,
}) => {
  const since = Date.now();
  await page.goto("/sign-in");
  await page.getByLabel("Email").fill(email);
  await page.getByRole("button", { name: "Email me a sign-in link" }).click();
  await expect(page.getByText("Check your email")).toBeVisible();

  let link: string | undefined;
  await expect
    .poll(async () => {
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
    })
    .toBeTruthy();

  await page.goto(link as string);
  await expect(page).toHaveURL(/\/welcome\/age$/);
});
