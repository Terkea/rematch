import { mkdir } from "node:fs/promises";
import AxeBuilder from "@axe-core/playwright";
import { type Browser, expect, type Locator, type Page, test } from "@playwright/test";
import { gotoSettled, hydrated, signInByMagicLink } from "./support/mail";

// The quality loop over the M4 surfaces (M4 Task 28): at 360px nothing scrolls sideways; axe
// (WCAG 2 A and AA) finds no serious or critical violation; and tabbing from the top of the page
// reaches the page's main action, which shows a visible focus outline. The signed-in routes use
// a stored state: `quality@` signs in once and never answers the age question, so /welcome/age
// stays on screen; the adult (`adult@`) answers it and holds /app/accounts. Game pages have their
// own loop (game-pages-quality.spec.ts).

const PENDING_STATE = "e2e/.auth/quality-pending.json";
const ADULT_STATE = "e2e/.auth/quality-adult.json";

type Who = "anonymous" | "pending" | "adult";

interface Route {
  url: string;
  who: Who;
  /** Hydrated before the checks run. */
  ready: string;
  /** The page's main action: where a keyboard user is headed. */
  main: (page: Page) => Locator;
}

const ROUTES: Route[] = [
  {
    url: "/styleguide?render=all",
    who: "anonymous",
    ready: "[data-section='charts'] button[aria-label]",
    main: (p) => p.locator("main").locator("a[href], button").first(),
  },
  {
    url: "/sign-in",
    who: "anonymous",
    ready: "form",
    main: (p) => p.getByRole("button", { name: "Email me a sign-in link" }),
  },
  {
    url: "/sign-in/check-email",
    who: "anonymous",
    ready: "main a[href='/sign-in']",
    main: (p) => p.getByRole("link", { name: "Use a different email" }),
  },
  {
    url: "/welcome/age",
    who: "pending",
    ready: "form",
    main: (p) => p.getByRole("button", { name: "Continue" }),
  },
  {
    // The age gate's end: no action of its own, so the way out is the brand link home.
    url: "/blocked",
    who: "anonymous",
    ready: "a[href='/']",
    main: (p) => p.locator("a[href='/']").first(),
  },
  {
    url: "/app/accounts",
    who: "adult",
    ready: "form",
    main: (p) => p.getByRole("button", { name: "Connect chess.com" }),
  },
  {
    url: "/lab",
    who: "anonymous",
    ready: "main a[href^='/lab/']",
    main: (p) => p.locator("main a[href^='/lab/']").first(),
  },
];

/** Signs `email` in, optionally answers the age question, and stores the session. */
async function storeState(
  browser: Browser,
  email: string,
  path: string,
  answerAge: boolean,
): Promise<void> {
  const context = await browser.newContext();
  const page = await context.newPage();
  await signInByMagicLink(page, page.request, email);
  await expect(page).toHaveURL(/\/welcome\/age$/);
  if (answerAge) {
    await hydrated(page);
    await page.getByLabel("Month").selectOption("6");
    await page.getByLabel("Year").selectOption("1990");
    await page.getByRole("button", { name: "Continue" }).click();
    await expect(page).toHaveURL(/\/app\/accounts$/, { timeout: 120_000 });
  }
  await mkdir("e2e/.auth", { recursive: true });
  await context.storageState({ path });
  await context.close();
}

test.describe("quality", () => {
  test.describe.configure({ mode: "serial" });

  test.beforeAll(async ({ browser }, info) => {
    if (info.project.name !== "e2e") return;
    test.setTimeout(300_000);
    await storeState(browser, "quality@example.invalid", PENDING_STATE, false);
    await storeState(browser, "quality-adult@example.invalid", ADULT_STATE, true);
  });

  for (const route of ROUTES) {
    test(`${route.url}: no sideways scroll at 360px, no serious axe finding, visible focus on the main action`, async ({
      browser,
    }, info) => {
      test.skip(info.project.name !== "e2e", "runs once, in the e2e project");
      test.setTimeout(300_000);
      const context = await browser.newContext({
        viewport: { width: 360, height: 800 },
        ...(route.who === "pending" ? { storageState: PENDING_STATE } : {}),
        ...(route.who === "adult" ? { storageState: ADULT_STATE } : {}),
      });
      const page = await context.newPage();
      try {
        await gotoSettled(page, route.url);
        expect(new URL(page.url()).pathname, "the route stays put").toBe(
          new URL(route.url, page.url()).pathname,
        );
        await hydrated(page, route.ready);
        await page.evaluate(() => document.fonts.ready);

        const width = await page.evaluate(() => document.documentElement.scrollWidth);
        expect(width, "no horizontal scroll at 360px").toBeLessThanOrEqual(360);

        const axe = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
        const bad = axe.violations
          .filter((v) => v.impact === "serious" || v.impact === "critical")
          .map(
            (v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
          );
        info.annotations.push({
          type: "axe",
          description: `${axe.passes.length} rules passed; violations: ${
            axe.violations.map((v) => `${v.id} (${v.impact})`).join(", ") || "none"
          }`,
        });
        console.log(`axe ${route.url}: ${info.annotations.at(-1)?.description}`);
        expect(bad, "axe serious or critical violations").toEqual([]);

        // Tab from the top until the main action has focus; it must show an outline.
        const main = route.main(page);
        await expect(main).toBeVisible();
        await page.evaluate(() => {
          (document.activeElement as HTMLElement | null)?.blur();
          window.scrollTo(0, 0);
        });
        let reached = false;
        for (let i = 0; i < 40 && !reached; i++) {
          await page.keyboard.press("Tab");
          reached = await main.evaluate((el) => el === document.activeElement);
        }
        expect(reached, "tabbing from the top reaches the main action").toBe(true);
        const outline = await main.evaluate((el) => getComputedStyle(el).outlineStyle);
        expect(outline, "the main action shows a focus outline").not.toBe("none");
      } finally {
        await context.close();
      }
    });
  }
});
