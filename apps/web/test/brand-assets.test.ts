import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  BRAND_INK,
  BRAND_NIGHT_INK,
  BRAND_NIGHT_PENCIL,
  BRAND_PAPER,
  BRAND_PENCIL,
  cardHtml,
  iconSvg,
  lockupHtml,
} from "../scripts/brand-assets-lib.js";
import manifest from "../src/app/manifest.js";

// The Rematch logo (docs/superpowers/specs/2026-11-23-rematch-brand-design.md): the boy with his
// board, ink only. `pnpm --filter @ci/web brand` regenerates every file checked here from
// packages/ui/assets/brand.

const HERE = dirname(fileURLToPath(import.meta.url));
const WEB = join(HERE, "..");
const ROOT = join(WEB, "../..");
const BRAND = join(ROOT, "packages/ui/assets/brand");

/** Width and height from a PNG's IHDR chunk. */
function pngSize(buf: Buffer): [number, number] {
  expect(buf.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
  expect(buf.toString("ascii", 12, 16)).toBe("IHDR");
  return [buf.readUInt32BE(16), buf.readUInt32BE(20)];
}

const PEN = /#c8372d|#d3312b|--color-pen/i;

/** A colour token's value in one theme block of tokens.css (`:root` day, or the `[data-theme="dark"]` block). */
function token(name: string, theme: "day" | "night"): string {
  const css = readFileSync(join(ROOT, "packages/ui/src/styles/tokens.css"), "utf8");
  const block = theme === "day" ? css : css.slice(css.indexOf('\n[data-theme="dark"] {'));
  const value = block.match(new RegExp(`--color-${name}:\\s*(#[0-9a-f]{6});`, "i"))?.[1];
  if (!value) throw new Error(`tokens.css has no --color-${name} (${theme})`);
  return value.toUpperCase();
}

describe("the brand colours are the app's tokens", () => {
  it("draws in day ink and pencil on day paper", () => {
    expect(BRAND_INK).toBe(token("ink", "day"));
    expect(BRAND_PENCIL).toBe(token("pencil", "day"));
    expect(BRAND_PAPER).toBe(token("paper", "day"));
  });
  it("draws in night ink and pencil on dark chrome", () => {
    expect(BRAND_NIGHT_INK).toBe(token("ink", "night"));
    expect(BRAND_NIGHT_PENCIL).toBe(token("pencil", "night"));
  });
  it("is the cool scoresheet, not the withdrawn warm palette", () => {
    expect(BRAND_PAPER).toBe("#F3F5F8");
    expect(BRAND_INK).toBe("#16233B");
  });
});

describe("the favicon set", () => {
  it("serves icon.svg: the 16 px icon in ink, pale night ink on dark browser chrome, never red", () => {
    const svg = readFileSync(join(WEB, "src/app/icon.svg"), "utf8");
    expect(svg).toBe(iconSvg(readFileSync(join(BRAND, "rematch-icon-16.svg"), "utf8")));
    expect(svg).toContain(`color:${BRAND_INK}`);
    expect(svg).toMatch(/@media \(prefers-color-scheme: ?dark\)\{svg\{color:#E7ECF4\}\}/);
    expect(svg).toContain("<title>Rematch</title>");
    expect(svg).not.toMatch(PEN);
  });

  it("ships favicon.ico with 16, 32 and 48 px PNG images", () => {
    const ico = readFileSync(join(WEB, "src/app/favicon.ico"));
    expect(ico.readUInt16LE(2)).toBe(1);
    expect(ico.readUInt16LE(4)).toBe(3);
    const sizes = [0, 1, 2].map((i) => {
      const offset = ico.readUInt32LE(6 + i * 16 + 12);
      return pngSize(ico.subarray(offset));
    });
    expect(sizes).toEqual([
      [16, 16],
      [32, 32],
      [48, 48],
    ]);
  });

  it("ships a 180 px apple-touch icon", () => {
    expect(pngSize(readFileSync(join(WEB, "src/app/apple-icon.png")))).toEqual([180, 180]);
  });

  it("lists 192 and 512 px icons in the web manifest, and they exist", () => {
    const m = manifest();
    expect(m.name).toBe("Rematch, chess improver");
    expect(m.short_name).toBe("Rematch");
    const icons = m.icons ?? [];
    expect(icons.map((i) => i.sizes)).toEqual(["192x192", "512x512"]);
    for (const icon of icons) {
      const file = join(WEB, "public", icon.src);
      expect(existsSync(file), icon.src).toBe(true);
      const n = Number(icon.sizes?.split("x")[0]);
      expect(pngSize(readFileSync(file))).toEqual([n, n]);
    }
  });

  it("copies public/ into the production image, so the manifest icons are served", () => {
    expect(readFileSync(join(WEB, "Dockerfile"), "utf8")).toContain(
      "/app/apps/web/public ./apps/web/public",
    );
  });
});

describe("the icons by size", () => {
  // 16 px is pixel art on a 16 px grid (whole-unit rectangles, so a tab draws it without blur).
  // From 32 px up it's the fine line drawing (the pixel figure read as a cartoon at 32/48).
  it("rematch-icon-16.svg sits on a 16 px grid, ink only", () => {
    const svg = readFileSync(join(BRAND, "rematch-icon-16.svg"), "utf8");
    expect(svg).toContain('viewBox="0 0 16 16"');
    expect(svg).toContain('fill="currentColor"');
    expect(svg).not.toMatch(/#[0-9a-f]{3,6}(?![0-9a-z])/i);
    const ds = [...svg.matchAll(/ d="([^"]+)"/g)].map((m) => m[1] ?? "");
    expect(ds.length).toBeGreaterThan(0);
    for (const d of ds) expect(d).toMatch(/^(M\d+ \d+h\d+v1h-\d+z)+$/);
  });

  it("rematch-icon.svg is the fine line drawing, in currentColor", () => {
    const svg = readFileSync(join(BRAND, "rematch-icon.svg"), "utf8");
    expect(svg).toContain('viewBox="0 0 32 32"');
    expect(svg).toContain('fill="currentColor"');
    expect(svg).toMatch(/ d="[^"]*[Cc]/);
  });
});

describe("the lockup", () => {
  // The owner's pick (lockup-type option 4): "Rematch" in Literata italic with a capital R, and
  // "chess improver" in a smaller Literata italic, in pencil, flush with the end of the word.
  const faces = "/* faces */";
  for (const [name, html] of [
    ["README", lockupHtml("<svg/>", faces, BRAND_INK, BRAND_PENCIL)],
    ["social card", cardHtml("<svg/>", faces, 1200, 630)],
  ] as const) {
    it(`sets the ${name} in Literata italic, capital R, italic tagline`, () => {
      expect(html).toContain(">Rematch</p>");
      expect(html).toContain(">chess improver</p>");
      expect(html).not.toMatch(/>rematch</);
      expect(html).not.toMatch(/lowercase|Schibsted/);
      expect(html.match(/font-family:'Literata Italic'/g)?.length).toBeGreaterThanOrEqual(2);
      expect(html).toContain("align-items:flex-end");
    });
  }

  it("draws the README tagline in night pencil on dark", () => {
    expect(lockupHtml("<svg/>", faces, BRAND_NIGHT_INK, BRAND_NIGHT_PENCIL)).toContain(
      BRAND_NIGHT_PENCIL,
    );
  });
});

describe("the header", () => {
  it("has no generated icon module: headers carry the wordmark alone", () => {
    expect(existsSync(join(WEB, "src/components/brand/icon.generated.ts"))).toBe(false);
  });
});
