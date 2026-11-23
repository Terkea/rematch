import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import {
  BRAND_INK,
  BRAND_NIGHT_INK,
  BRAND_NIGHT_PENCIL,
  BRAND_PAPER,
  BRAND_PENCIL,
  cardHtml,
  icoFile,
  iconSvg,
  lockupHtml,
} from "./brand-assets-lib";

// Every file made from the Rematch logo (packages/ui/assets/brand), in one run:
//
//   pnpm --filter @ci/web brand
//
// The rasters are drawn by the system Chrome through Playwright (a dev dependency), so the logo
// needs no image library. test/brand-assets.test.ts checks the outputs.

const WEB = join(dirname(fileURLToPath(import.meta.url)), "..");
const ROOT = join(WEB, "../..");
const BRAND = join(ROOT, "packages/ui/assets/brand");
const FONTS = join(WEB, "src/app/fonts");

const icon = readFileSync(join(BRAND, "rematch-icon.svg"), "utf8");
// The tab icon is drawn on a 16 px grid of its own, so a browser tab shows it without blur.
const icon16 = readFileSync(join(BRAND, "rematch-icon-16.svg"), "utf8");
const mark = readFileSync(join(BRAND, "rematch-mark.svg"), "utf8");

const write = (path: string, data: string | Buffer) => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
  console.log(`brand: wrote ${path.slice(ROOT.length + 1)}`);
};

// Text files.
write(join(WEB, "src/app/icon.svg"), iconSvg(icon16));
write(join(ROOT, "site/public/favicon.svg"), iconSvg(icon16));

// Rasters.
const font = (file: string) =>
  `data:font/woff2;base64,${readFileSync(join(FONTS, file)).toString("base64")}`;
const FACES = `
@font-face { font-family: "Literata Italic"; src: url(${font("Literata-Italic-VF.woff2")}); font-style: italic; font-weight: 200 900; }
@font-face { font-family: "Literata"; src: url(${font("Literata-VF.woff2")}); font-weight: 200 900; }
`;
const sized = (svg: string, attrs: string) => svg.replace("<svg", `<svg ${attrs}`);

/** The icon on an opaque paper tile, `inset` of the side left around it, corners `radius`. */
const tile = (size: number, inset: number, radius: number, source = icon, crisp = false) => `
<body style="margin:0;background:transparent">
<div style="width:${size}px;height:${size}px;background:${BRAND_PAPER};color:${BRAND_INK};border-radius:${radius * size}px;display:grid;place-items:center">
${sized(source, `width="${Math.round(size * (1 - 2 * inset))}" height="${Math.round(size * (1 - 2 * inset))}"${crisp ? ' shape-rendering="crispEdges"' : ""}`)}
</div></body>`;

async function main() {
  const browser = await chromium.launch({ channel: "chrome" });
  try {
    const shot = async (html: string, w: number, h: number, omitBackground: boolean, scale = 1) => {
      const page = await browser.newPage({
        viewport: { width: w, height: h },
        deviceScaleFactor: scale,
      });
      await page.setContent(html);
      await page.evaluate(() => document.fonts.ready);
      const png = await page.screenshot({
        omitBackground,
        clip: { x: 0, y: 0, width: w, height: h },
      });
      await page.close();
      return png;
    };
    // The tab icon fallback: at 16 px the pixel drawing, filling the tile on whole pixels; from
    // 32 px the fine line drawing, with a little air round it.
    const ico = [];
    for (const size of [16, 32, 48])
      ico.push({
        size,
        png: await shot(
          size === 16 ? tile(16, 0, 0.18, icon16, true) : tile(size, 0.03, 0.18),
          size,
          size,
          true,
        ),
      });
    write(join(WEB, "src/app/favicon.ico"), icoFile(ico));
    // iOS rounds the corners itself; the manifest icons keep the maskable safe zone.
    write(join(WEB, "src/app/apple-icon.png"), await shot(tile(180, 0.14, 0), 180, 180, false));
    for (const size of [192, 512]) {
      write(
        join(WEB, `public/icon-${size}.png`),
        await shot(tile(size, 0.18, 0), size, size, false),
      );
    }
    write(
      join(ROOT, "site/public/og.png"),
      await shot(cardHtml(mark, FACES, 1200, 630), 1200, 630, false),
    );
    write(
      join(ROOT, "site/public/media/github-social-preview.png"),
      await shot(cardHtml(mark, FACES, 1280, 640), 1280, 640, false),
    );
    // GitHub shows the README's <picture> by the reader's colour scheme; drawn at 2x for retina.
    write(
      join(ROOT, "docs/media/rematch-lockup-day.png"),
      await shot(lockupHtml(mark, FACES, BRAND_INK, BRAND_PENCIL), 640, 220, true, 2),
    );
    write(
      join(ROOT, "docs/media/rematch-lockup-night.png"),
      await shot(lockupHtml(mark, FACES, BRAND_NIGHT_INK, BRAND_NIGHT_PENCIL), 640, 220, true, 2),
    );
  } finally {
    await browser.close();
  }
}

await main();
