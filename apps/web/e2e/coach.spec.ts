import { expect, type Page, test } from "@playwright/test";
import {
  latestAnswer,
  latestRun,
  linkPeer,
  seedCoach,
  simulate,
  spendAllowance,
} from "./support/coach";
import { adultPage } from "./support/game-pages";
import { enterAge, gotoSettled, signInByMagicLink } from "./support/mail";

// The coach surfaces end to end (M5b Task 11, ADR 0074), on the synthetic coach seed
// (e2e/helpers/seed-coach.ts). No worker runs and no AI is called: `simulate` plays the worker's
// part, writing the rows and events the R3 jobs write. The steps share one seeded world and run
// in order (a rebuild in step 2 is what step 5's account event rides on).

const PEER = "coach-peer@example.invalid";
const DRAFT_1 = "Most of the misses ";
const DRAFT_2 = "come after a capture.";
const VALIDATED =
  "Most of your missed tactics come right after a capture: you recapture at once instead of checking forcing moves first.";

/** Waits until the element is hydrated (React props attached), like the game-page specs. */
async function ready(page: Page, url: string, selector: string): Promise<void> {
  await gotoSettled(page, url);
  await page.waitForFunction(
    (sel) => {
      const el = document.querySelector(sel);
      return el !== null && Object.keys(el).some((key) => key.startsWith("__reactProps"));
    },
    selector,
    { timeout: 180_000 },
  );
}

/** Opens a page whose event stream is live (its `/api/events` answered). */
async function live(page: Page, url: string, selector: string): Promise<void> {
  const opened = page.waitForResponse((r) => new URL(r.url()).pathname === "/api/events", {
    timeout: 180_000,
  });
  await ready(page, url, selector);
  await opened;
}

test.describe
  .serial("the coach surfaces", () => {
    test.setTimeout(600_000);

    let page: Page;
    let seed: ReturnType<typeof seedCoach>;

    test.beforeAll(async ({ browser, request }, info) => {
      test.skip(info.project.name !== "e2e", "runs in the e2e project");
      test.setTimeout(600_000);
      page = await adultPage(browser, request);
      seed = seedCoach();
    });

    test.afterAll(async () => {
      await page?.context().close();
    });

    test("1. the report shows the snapshot, seven areas, ranked weaknesses and the disclosure", async () => {
      await ready(page, "/app/report", "[data-report-version]");
      await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your coaching report");
      const snapshot = page.locator("section", { has: page.locator("#report-snapshot") });
      await expect(page.getByRole("heading", { name: "Where you stand" })).toBeVisible();
      await expect(page.getByRole("heading", { name: "Your seven areas" })).toBeVisible();
      for (const area of ["Openings", "Middlegame", "Endgame", "Tactics", "Calculation", "Time"]) {
        await expect(snapshot.getByText(area, { exact: true }).first()).toBeVisible();
      }
      await expect(page.getByText("Mindset is coaching advice, not a score").first()).toBeVisible();
      await expect(page.getByRole("heading", { name: "What's costing you games" })).toBeVisible();
      // Boards from the report's positions: at least one drawn board in the weaknesses section.
      const weaknesses = page.locator("section:has(#report-weaknesses)");
      await expect(weaknesses.locator("svg[role='img']").first()).toBeVisible();
      await expect(page.getByText("Written by AI").first()).toBeVisible();
    });

    test("2. a rebuild shows the building state, and sections arrive without a reload", async () => {
      await live(page, "/app/report", "[data-report-version]");
      const rebuild = page.getByRole("button", { name: "Rebuild report" });
      await rebuild.click();
      await expect(page.getByText(/Building your report: 0 of 5 sections written/)).toBeVisible();
      const runId = latestRun();

      // A marker on the window survives client-side refreshes, not a reload.
      await page.evaluate(() => {
        (window as unknown as { __noReload: boolean }).__noReload = true;
      });
      simulate({ kind: "report-section", runId, section: "overview" });
      await expect(page.getByText(/Building your report: 1 of 5 sections written/)).toBeVisible();
      await expect(page.locator("#report-overview")).toBeVisible();
      simulate({ kind: "report-section", runId, section: "weaknesses" });
      await expect(page.getByText(/Building your report: 2 of 5 sections written/)).toBeVisible();
      await expect(page.locator("#report-weaknesses")).toBeVisible();
      expect(
        await page.evaluate(() => (window as unknown as { __noReload?: boolean }).__noReload),
        "no reload",
      ).toBe(true);
    });

    test("3. a weakness opens its detail; asking about it carries the anchor", async () => {
      await ready(page, "/app/weaknesses", "main h1");
      await page.locator("a[href*='/app/weaknesses/W3.shallow']").first().click();
      await page.waitForURL(/\/app\/weaknesses\/W3\.shallow/);
      await expect(page.getByRole("heading", { name: "State over time" })).toBeVisible();
      await expect(page.getByText(/22 of 120 chances/).first()).toBeVisible();
      await expect(page.getByRole("heading", { name: "Example positions" })).toBeVisible();
      const examples = page.locator("section:has(#examples-heading)");
      await expect(examples.locator("svg[role='img']").first()).toBeVisible();

      await page.getByRole("link", { name: "Ask your coach about this" }).click();
      await page.waitForURL(/\/app\/coach\/new\?/);
      await expect(page.getByText("About a weakness").first()).toBeVisible();
    });

    test("4. a question streams in order, survives a reload, and the validated answer replaces it", async () => {
      await live(page, "/app/coach/new", "textarea");
      await page.getByRole("textbox").fill("Why do I miss tactics after captures?");
      await page.getByRole("button", { name: "Send" }).click();
      await page.waitForURL(/\/app\/coach\/[0-9a-f-]{36}$/);
      await expect(page.getByText("Your coach is reading your games").first()).toBeVisible();
      const messageId = latestAnswer();

      simulate({ kind: "chat-chunk", messageId, seq: 0, text: DRAFT_1 });
      await expect(page.getByText(DRAFT_1.trim())).toBeVisible();

      // A reload between the chunks keeps what streamed (stored chunks, R5).
      await live(page, page.url(), "textarea");
      await expect(page.getByText(DRAFT_1.trim())).toBeVisible();

      simulate({ kind: "chat-chunk", messageId, seq: 1, text: DRAFT_2 });
      const answer = page.locator("article[data-role='assistant']").last();
      await expect(answer).toContainText(`${DRAFT_1}${DRAFT_2}`);

      simulate({ kind: "chat-done", messageId, replace: true });
      await expect(answer).toContainText(VALIDATED);
      await expect(answer).not.toContainText(DRAFT_2);
      await expect(answer.getByText("Written by AI").first()).toBeVisible();
    });

    test("5. another user of the account gets not-found and none of the owner's chunks", async ({
      browser,
      request,
    }) => {
      const context = await browser.newContext();
      const peer = await context.newPage();
      // The page's event log: every event its EventSource delivers, by type.
      await peer.addInitScript(() => {
        const log: string[] = [];
        (window as unknown as { __events: string[] }).__events = log;
        const add = EventSource.prototype.addEventListener;
        EventSource.prototype.addEventListener = function (
          this: EventSource,
          type: string,
          listener: EventListenerOrEventListenerObject,
          options?: boolean | AddEventListenerOptions,
        ) {
          if (type !== "open" && type !== "error") {
            add.call(this, type, () => log.push(type));
          }
          return add.call(this, type, listener, options);
        } as typeof EventSource.prototype.addEventListener;
      });
      await signInByMagicLink(peer, request, PEER);
      if (new URL(peer.url()).pathname === "/welcome/age") await enterAge(peer, 1990, 6);
      linkPeer(PEER);

      await live(peer, `/app/coach/${seed.threadId}`, "main");
      await expect(peer.getByText("We couldn't find that conversation.").first()).toBeVisible();

      // The owner's answer streams while the peer listens; an account event follows, so the
      // peer's stream is shown to be live.
      simulate({
        kind: "chat-chunk",
        messageId: seed.streamingMessageId,
        seq: 0,
        text: "Private.",
      });
      simulate({ kind: "report-section", runId: latestRun(), section: "areas" });
      const events = () =>
        peer.evaluate(() => [...new Set((window as unknown as { __events: string[] }).__events)]);
      await expect.poll(events, { timeout: 30_000 }).toContain("report.section.ready");
      expect(await events()).not.toContain("chat.chunk");
      await context.close();
    });

    test("6. a weekly_cap denial shows the cap copy and disables the composer", async () => {
      // With step 4's answer, 99 of the week's 100 are used; another device uses the last while
      // this question waits.
      spendAllowance(98);
      await live(page, "/app/coach/new", "textarea");
      await page.getByRole("textbox").fill("One more question about my endgames?");
      await page.getByRole("button", { name: "Send" }).click();
      await page.waitForURL(/\/app\/coach\/[0-9a-f-]{36}$/);
      const messageId = latestAnswer();
      spendAllowance(1);
      simulate({ kind: "chat-failed", messageId, reason: "weekly_cap" });
      await expect(
        page.getByText("You've used this week's coach messages. They reset on Monday.").first(),
      ).toBeVisible();
      await expect(page.getByRole("textbox")).toBeDisabled();
    });

    test("7. mindset says it has no score; an unknown area is not found", async () => {
      await ready(page, "/app/areas/mindset", "main h1");
      await expect(page.getByText("Mindset is coaching advice, not a score").first()).toBeVisible();
      const res = await page.goto("/app/areas/kingside");
      expect(res?.status()).toBe(404);
    });

    test("8. progress shows before-and-after first, a KPI gap for the empty week, the review", async () => {
      await ready(page, "/app/progress", "main h1");
      const headings = await page.locator("main h2").allInnerTexts();
      expect(headings[0]).toBe("Before and after");
      // The null week is a stated gap in the grid's summary, never a zero.
      await expect(
        page.locator("[data-kpi='accuracy']").getByText(/no \w+ in week of 5 Oct 2026/),
      ).toBeAttached();
      await expect(page.getByRole("heading", { name: "Weekly reviews" })).toBeVisible();
      await expect(page.getByText("A steadier week.").first()).toBeVisible();
    });

    test("9. the command menu finds Endgame and opens its area", async () => {
      await ready(page, "/app", "main");
      await page.keyboard.press("Control+k");
      const dialog = page.getByRole("dialog");
      await expect(dialog).toBeVisible();
      await dialog.getByRole("combobox").fill("Endgame");
      await expect(dialog.getByRole("option", { name: "Endgame" })).toBeVisible();
      await page.keyboard.press("Enter");
      await page.waitForURL(/\/app\/areas\/endgame$/);
    });
  });
