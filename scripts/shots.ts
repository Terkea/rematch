import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname } from "node:path";
import { parseShotsArgs, type Shot, shotMatrix } from "./shots-lib.js";

// Playwright is a dependency of @ci/web only, so resolve it from there.
const { chromium } = createRequire(new URL("../apps/web/package.json", import.meta.url))(
  "@playwright/test",
) as typeof import("@playwright/test");

const heights = { 390: 844, 1440: 900 } as const;
const FRAMES = 12;
const FRAME_MS = 100;
const HIDE_CHROME = "[data-lab-chrome]{display:none !important}";

const args = parseShotsArgs(process.argv.slice(2));
let failed = false;

function fail(shot: Shot, reason: string): void {
  failed = true;
  const text = reason.split("\n")[0] ?? reason;
  console.error(`FAIL ${shot.route} (${shot.width} ${shot.theme}) ${text}`);
}

const browser = await chromium.launch();
try {
  for (const shot of shotMatrix(args.out, args.routes)) {
    const context = await browser.newContext({
      viewport: { width: shot.width, height: heights[shot.width] },
      colorScheme: shot.theme,
      reducedMotion: args.motion ? "no-preference" : "reduce",
      ...(args.storage ? { storageState: args.storage } : {}),
    });
    try {
      const page = await context.newPage();
      // Pin next-themes to the requested theme, whatever the stored choice.
      await page.addInitScript((theme) => {
        try {
          localStorage.setItem("theme", theme);
        } catch {
          // storage unavailable: the colorScheme emulation still applies
        }
      }, shot.theme);
      const url = new URL(shot.route, args.baseUrl).toString();
      const response = await page.goto(url, { waitUntil: "networkidle" });
      const status = response?.status() ?? 0;
      if (status >= 400) {
        fail(shot, `HTTP ${status}`);
        continue;
      }
      await page.evaluate(() => document.fonts.ready);
      await page.addStyleTag({ content: HIDE_CHROME });
      await mkdir(dirname(shot.path), { recursive: true });
      if (args.motion) {
        // Reload so the entrance choreography starts from its first frame, with chrome hidden
        // before that frame. Viewport frames keep the 100 ms spacing (full-page capture is slower).
        await page.addInitScript((css) => {
          document.addEventListener("DOMContentLoaded", () => {
            const style = document.createElement("style");
            style.textContent = css;
            document.head.append(style);
          });
        }, HIDE_CHROME);
        const reloaded = await page.reload({ waitUntil: "domcontentloaded" });
        const reloadStatus = reloaded?.status() ?? 0;
        if (reloadStatus >= 400) {
          fail(shot, `HTTP ${reloadStatus} on reload`);
          continue;
        }
        for (let i = 0; i < FRAMES; i++) {
          const frame = shot.path.replace(/\.png$/, `-f${String(i).padStart(2, "0")}.png`);
          await page.screenshot({ path: frame });
          console.log(frame);
          await page.waitForTimeout(FRAME_MS);
        }
      } else {
        await page.screenshot({ path: shot.path, fullPage: true, animations: "disabled" });
        console.log(shot.path);
      }
    } catch (error) {
      fail(shot, error instanceof Error ? error.message : String(error));
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
}
if (failed) process.exit(1);
