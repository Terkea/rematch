import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import {
  BRAND_INK,
  BRAND_NIGHT_INK,
  BRAND_PAPER,
  BRAND_PENCIL,
  icoFile,
  iconModule,
  iconSvg,
  solidSvg,
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
const mark = readFileSync(join(BRAND, "rematch-mark.svg"), "utf8");

const write = (path: string, data: string | Buffer) => {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, data);
  console.log(`brand: wrote ${path.slice(ROOT.length + 1)}`);
};

// Text files.
write(join(WEB, "src/app/icon.svg"), iconSvg(icon));
write(join(ROOT, "site/public/favicon.svg"), iconSvg(icon));
write(join(WEB, "src/components/brand/icon.generated.ts"), iconModule(icon));

const iconTitle = "Rematch";
write(join(ROOT, "site/src/assets/rematch-icon-day.svg"), solidSvg(icon, BRAND_INK, iconTitle));
write(
  join(ROOT, "site/src/assets/rematch-icon-night.svg"),
  solidSvg(icon, BRAND_NIGHT_INK, iconTitle),
);

// Rasters.
const font = (file: string) =>
  `data:font/woff2;base64,${readFileSync(join(FONTS, file)).toString("base64")}`;
const FACES = `
@font-face { font-family: "Literata Italic"; src: url(${font("Literata-Italic-VF.woff2")}); font-style: italic; font-weight: 200 900; }
@font-face { font-family: "Literata"; src: url(${font("Literata-VF.woff2")}); font-weight: 200 900; }
@font-face { font-family: "Schibsted Grotesk"; src: url(${font("SchibstedGrotesk-VF.woff2")}); font-weight: 400 900; }`;
const sized = (svg: string, attrs: string) => svg.replace("<svg", `<svg ${attrs}`);

/** The icon on an opaque paper tile, `inset` of the side left around it, corners `radius`. */
const tile = (size: number, inset: number, radius: number) => `
<body style="margin:0;background:transparent">
<div style="width:${size}px;height:${size}px;background:${BRAND_PAPER};color:${BRAND_INK};border-radius:${radius * size}px;display:grid;place-items:center">
${sized(icon, `width="${Math.round(size * (1 - 2 * inset))}" height="${Math.round(size * (1 - 2 * inset))}"`)}
</div></body>`;

/** The social card: the full mark beside the lockup and the promise. */
const card = (w: number, h: number) => `
<style>${FACES}</style>
<body style="margin:0;width:${w}px;height:${h}px;background:${BRAND_PAPER};color:${BRAND_INK};display:flex;align-items:center;justify-content:center;gap:${w * 0.05}px">
${sized(mark, `height="${Math.round(h * 0.74)}" style="display:block;flex:none"`)}
<div style="display:flex;flex-direction:column">
  <p style="margin:0;font-family:'Literata Italic';font-style:italic;font-weight:500;font-size:${h * 0.25}px;line-height:1;letter-spacing:-0.01em">rematch</p>
  <p style="margin:${h * 0.02}px 0 0 ${h * 0.012}px;font-family:'Schibsted Grotesk';font-weight:500;font-size:${h * 0.062}px;color:${BRAND_PENCIL}">chess improver</p>
  <p style="margin:${h * 0.075}px 0 0 ${h * 0.008}px;font-family:'Literata';font-weight:400;font-size:${h * 0.07}px">Stop losing to the same move.</p>
</div></body>`;

/** The README lockup on a transparent ground: the mark, "rematch" and "chess improver" beneath. */
const lockup = (ink: string, sub: string) => `
<style>${FACES}</style>
<body style="margin:0;width:640px;height:220px;background:transparent;color:${ink};display:flex;align-items:center;gap:28px">
${sized(mark, 'height="208" style="display:block;flex:none"')}
<div style="display:flex;flex-direction:column">
  <p style="margin:0;font-family:'Literata Italic';font-style:italic;font-weight:500;font-size:104px;line-height:1;letter-spacing:-0.01em">rematch</p>
  <p style="margin:6px 0 0 6px;font-family:'Schibsted Grotesk';font-weight:500;font-size:30px;color:${sub}">chess improver</p>
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
    // The tab icon fallback: small sizes keep almost the whole tile for the boy.
    const ico = [];
    for (const size of [16, 32, 48])
      ico.push({ size, png: await shot(tile(size, 0.03, 0.18), size, size, true) });
    write(join(WEB, "src/app/favicon.ico"), icoFile(ico));
    // iOS rounds the corners itself; the manifest icons keep the maskable safe zone.
    write(join(WEB, "src/app/apple-icon.png"), await shot(tile(180, 0.14, 0), 180, 180, false));
    for (const size of [192, 512]) {
      write(
        join(WEB, `public/icon-${size}.png`),
        await shot(tile(size, 0.18, 0), size, size, false),
      );
    }
    write(join(ROOT, "site/public/og.png"), await shot(card(1200, 630), 1200, 630, false));
    write(
      join(ROOT, "site/public/media/github-social-preview.png"),
      await shot(card(1280, 640), 1280, 640, false),
    );
    // GitHub shows the README's <picture> by the reader's colour scheme; drawn at 2x for retina.
    write(
      join(ROOT, "docs/media/rematch-lockup-day.png"),
      await shot(lockup(BRAND_INK, BRAND_PENCIL), 640, 220, true, 2),
    );
    write(
      join(ROOT, "docs/media/rematch-lockup-night.png"),
      await shot(lockup(BRAND_NIGHT_INK, BRAND_NIGHT_INK), 640, 220, true, 2),
    );
  } finally {
    await browser.close();
  }
}

await main();
