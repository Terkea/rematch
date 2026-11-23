import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { BRAND_INK, BRAND_NIGHT_INK, iconSvg } from "../scripts/brand-assets-lib.js";
import manifest from "../src/app/manifest.js";
import { REMATCH_ICON } from "../src/components/brand/icon.generated.js";

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

describe("the favicon set", () => {
  it("serves icon.svg: the icon in ink, night cream on dark browser chrome, never red", () => {
    const svg = readFileSync(join(WEB, "src/app/icon.svg"), "utf8");
    expect(svg).toBe(iconSvg(readFileSync(join(BRAND, "rematch-icon.svg"), "utf8")));
    expect(BRAND_INK).toBe("#1F2A3D");
    expect(BRAND_NIGHT_INK).toBe("#E8E2D6");
    expect(svg).toContain(`color:${BRAND_INK}`);
    expect(svg).toMatch(/@media \(prefers-color-scheme: ?dark\)\{svg\{color:#E8E2D6\}\}/);
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

describe("the header mark", () => {
  it("is generated from rematch-icon.svg and draws in currentColor", () => {
    const source = readFileSync(join(BRAND, "rematch-icon.svg"), "utf8");
    expect(REMATCH_ICON.viewBox).toBe("0 0 32 32");
    // Every path of the source is in the component, unchanged.
    for (const d of source.matchAll(/ d="([^"]+)"/g)) expect(REMATCH_ICON.body).toContain(d[1]);
    expect(REMATCH_ICON.body).toContain('fill="currentColor"');
    expect(REMATCH_ICON.body).not.toMatch(PEN);
  });
});
