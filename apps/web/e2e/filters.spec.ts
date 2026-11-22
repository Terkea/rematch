import { spawnSync } from "node:child_process";
import { expect, type Page, test } from "@playwright/test";
import { enterAge, gotoSettled, signInByMagicLink } from "./support/mail";

// The shared filter bar (T3, ADR 0083) end to end, on synthetic games with fixed days
// (e2e/support/seed-filters.ts): the filter URL round-trips through links and the GET forms, the
// overview compares two periods by the verdict rule, and a short period says how many more games
// it needs. The forms are plain GET forms, so none of this waits for hydration.

const EMAIL = "filters@example.invalid";

const desktop = (page: Page) => page.locator("[data-filter-bar] > div");

async function noBrokenNumbers(page: Page) {
  const text = (await page.locator("main").textContent()) ?? "";
  expect(text).not.toMatch(/NaN|Infinity|undefined/);
}

test("filters round-trip through the URL, and the overview compares periods honestly", async ({
  page,
  request,
}, info) => {
  test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
  test.setTimeout(420_000);
  await page.setViewportSize({ width: 1440, height: 1000 });

  await signInByMagicLink(page, request, EMAIL);
  await expect(page).toHaveURL(/\/welcome\/age$/);
  await enterAge(page, 1990, 6);
  await expect(page).toHaveURL(/\/app\/accounts$/);
  const seed = spawnSync(
    "node_modules/.bin/tsx",
    ["--conditions", "development", "e2e/support/seed-filters.ts", EMAIL],
    { stdio: "inherit", env: process.env },
  );
  expect(seed.status).toBe(0);

  // Links: colour and format write the canonical URL, and the page reads it back.
  await gotoSettled(page, "/app");
  const bar = desktop(page);
  await expect(
    bar.getByRole("group", { name: "Time control" }).getByRole("link", { name: /^Rapid/ }),
  ).toHaveAttribute("aria-current", "true");
  await bar.getByRole("group", { name: "Colour" }).getByRole("link", { name: "White" }).click();
  await expect(page).toHaveURL(/\/app\?color=white$/);
  const split = page.locator("[data-slot='colour-split']");
  await expect(split.locator("[data-colour]")).toHaveCount(1);
  await expect(split.locator("[data-colour='white']")).toContainText("22 rapid games");
  await bar.getByRole("group", { name: "Format" }).getByRole("link", { name: "10+0" }).click();
  await expect(page).toHaveURL(/\/app\?fmt=600%2B0&color=white$/);

  // The custom range is a GET form with native date inputs; it keeps the other filters.
  const period = bar.getByRole("group", { name: "Period" });
  await period.getByText("Custom", { exact: true }).click();
  await period.getByLabel("From").fill("2026-10-01");
  await period.getByLabel("To").fill("2026-10-31");
  await period.getByRole("button", { name: "Show these dates" }).click();
  await expect(page).toHaveURL(/fmt=600%2B0/);
  await expect(page).toHaveURL(/color=white/);
  await expect(page).toHaveURL(/from=2026-10-01&to=2026-10-31/);
  await page.reload();
  await expect(desktop(page).getByRole("group", { name: "Period" }).getByLabel("From")).toHaveValue(
    "2026-10-01",
  );
  await expect(
    desktop(page).getByRole("group", { name: "Colour" }).getByRole("link", { name: "White" }),
  ).toHaveAttribute("aria-current", "true");
  await noBrokenNumbers(page);

  // Compare periods: the toggle turns on this month against last; custom periods are a form.
  await gotoSettled(page, "/app");
  await desktop(page).getByRole("link", { name: "Compare periods" }).click();
  await expect(page).toHaveURL(/\/app\?cmp=month$/);
  const cmp = desktop(page).getByRole("group", { name: "Compare periods" });
  await cmp.getByText("Custom", { exact: true }).click();
  await cmp.getByLabel("A from").fill("2026-09-01");
  await cmp.getByLabel("A to").fill("2026-09-30");
  await cmp.getByLabel("B from").fill("2026-10-01");
  await cmp.getByLabel("B to").fill("2026-10-31");
  await cmp.getByRole("button", { name: "Compare these periods" }).click();
  await expect(page).toHaveURL(/cmp=custom/);
  await expect(page).toHaveURL(
    /a_from=2026-09-01&a_to=2026-09-30&b_from=2026-10-01&b_to=2026-10-31/,
  );

  const block = page.locator("[data-slot='compare']");
  await expect(block.getByRole("heading", { name: "Period A against period B" })).toBeVisible();
  await expect(block).toContainText("1 Sep to 30 Sep 2026, 20 games");
  await expect(block).toContainText("1 Oct to 31 Oct 2026, 20 games");
  const accuracy = block.locator("[data-row='accuracy']");
  await expect(accuracy.locator("[data-verdict]")).toHaveText("Better");
  await expect(accuracy).toContainText("20 analysed games");
  await expect(block.locator("[data-row='blunders'] [data-verdict]")).toHaveText("Better");
  await noBrokenNumbers(page);

  // A short period B: no change is called, and the page says how many more games it needs.
  await gotoSettled(
    page,
    "/app?cmp=custom&a_from=2026-09-01&a_to=2026-09-30&b_from=2026-11-01&b_to=2026-11-05",
  );
  const short = page.locator("[data-slot='compare'] [data-row='accuracy']");
  await expect(short.locator("[data-verdict]")).toHaveText(
    "Not enough games to tell: need 6 more in period B",
  );
  await expect(short).toContainText("Need 6 more games, 4 analysed games");
  await expect(page.locator("[data-slot='compare']")).not.toContainText(/Better|Worse/);
  await noBrokenNumbers(page);

  // The games page reads the shared contract; an old `colour` link moves to `color`.
  await gotoSettled(page, "/app/games?colour=black");
  await expect(page).toHaveURL(/\/app\/games\?color=black$/);
  await expect(
    desktop(page).getByRole("group", { name: "Colour" }).getByRole("link", { name: "Black" }),
  ).toHaveAttribute("aria-current", "true");

  // On a phone the bar is one disclosure that names the active filters.
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoSettled(page, "/app?color=black&cmp=month");
  const summary = page.locator("[data-filter-bar] > details > summary");
  await expect(summary).toContainText("Filters (2)");
  await expect(summary).toContainText("Rapid, Black, this month vs last");
  await summary.click();
  await expect(
    page.locator("[data-filter-bar] > details").getByRole("link", { name: "White" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    ),
  ).toBe(0);

  // Shots for the critique loop (DESIGN.md "CL"), signed in: the repo's `.shots/` (git-ignored).
  const compare =
    "/app?cmp=custom&a_from=2026-09-01&a_to=2026-09-30&b_from=2026-10-01&b_to=2026-10-31";
  const routes = [
    ["overview-compare", compare],
    ["overview-colour", "/app?fmt=600%2B0&from=2026-10-01&to=2026-10-31"],
    [
      "overview-short",
      compare.replace("b_from=2026-10-01&b_to=2026-10-31", "b_from=2026-11-01&b_to=2026-11-05"),
    ],
    ["games", "/app/games?color=black"],
    [
      "progress",
      "/app/progress?cmp=custom&a_from=2026-09-01&a_to=2026-09-30&b_from=2026-10-01&b_to=2026-10-31",
    ],
    ["weaknesses", "/app/weaknesses?color=white"],
  ] as const;
  // The system theme decides, so `emulateMedia` picks day or night.
  await page.addInitScript(() => {
    try {
      localStorage.removeItem("theme");
    } catch {
      // storage unavailable: the colour-scheme emulation still applies
    }
  });
  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: width === 390 ? 844 : 900 });
    for (const scheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
      for (const [name, url] of routes) {
        await gotoSettled(page, url);
        await page.evaluate(() => document.fonts.ready);
        await expect(page.locator("main")).toBeVisible();
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
          ),
          `${name} at ${width}`,
        ).toBe(0);
        await page.screenshot({
          path: `../../.shots/filters/${name}/${width}-${scheme}.png`,
          fullPage: true,
        });
      }
    }
  }
});
