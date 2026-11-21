import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname } from "node:path";
import { parseShotsArgs, shotMatrix } from "./shots-lib.js";

// Playwright is a dependency of @ci/web only, so resolve it from there.
const { chromium } = createRequire(new URL("../apps/web/package.json", import.meta.url))(
  "@playwright/test",
) as typeof import("@playwright/test");

const heights = { 390: 844, 1440: 900 } as const;
const FRAMES = 12;
const FRAME_MS = 100;
const HIDE_CHROME = "[data-lab-chrome]{display:none !important}";

const args = parseShotsArgs(process.argv.slice(2));
const browser = await chromium.launch();
let failed = false;

for (const shot of shotMatrix(args.out, args.routes)) {
  const context = await browser.newContext({
    viewport: { width: shot.width, height: heights[shot.width] },
    colorScheme: shot.theme,
    reducedMotion: args.motion ? "no-preference" : "reduce",
    ...(args.storage ? { storageState: args.storage } : {}),
  });
  const page = await context.newPage();
  // Pin next-themes to the requested theme, whatever the stored choice.
  await page.addInitScript((theme) => {
    try {
      localStorage.setItem("theme", theme);
    } catch {
      // storage unavailable: the colorScheme emulation still applies
    }
  }, shot.theme);
  const response = await page.goto(new URL(shot.route, args.baseUrl).toString(), {
    waitUntil: "networkidle",
  });
  const status = response?.status() ?? 0;
  if (status >= 400) {
    failed = true;
    console.error(`FAIL ${status} ${shot.route} (${shot.width} ${shot.theme})`);
    await context.close();
    continue;
  }
  await page.evaluate(() => document.fonts.ready);
  await page.addStyleTag({ content: HIDE_CHROME });
  await mkdir(dirname(shot.path), { recursive: true });
  if (args.motion) {
    // Reload so the entrance choreography is captured from its first frame.
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.addStyleTag({ content: HIDE_CHROME });
    for (let i = 0; i < FRAMES; i++) {
      const frame = shot.path.replace(/\.png$/, `-f${String(i).padStart(2, "0")}.png`);
      await page.screenshot({ path: frame, fullPage: true });
      console.log(frame);
      await page.waitForTimeout(FRAME_MS);
    }
  } else {
    await page.screenshot({ path: shot.path, fullPage: true });
    console.log(shot.path);
  }
  await context.close();
}

await browser.close();
if (failed) process.exit(1);
