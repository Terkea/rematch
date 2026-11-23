import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

// The Rematch logo on the site (made by `pnpm --filter @ci/web brand`): the same tab icon as the
// app, the lockup in the header, and the social card as og:image.

const SITE = resolve(import.meta.dirname, "..");
const ROOT = resolve(SITE, "..");
const read = (p: string) => readFileSync(resolve(ROOT, p));

function pngSize(buf: Buffer): [number, number] {
  expect(buf.toString("ascii", 1, 4)).toBe("PNG");
  return [buf.readUInt32BE(16), buf.readUInt32BE(20)];
}

describe("the site's brand", () => {
  it("uses the app's tab icon", () => {
    expect(read("site/public/favicon.svg").equals(read("apps/web/src/app/icon.svg"))).toBe(true);
  });

  it("has a 1200 x 630 social card and a 1280 x 640 GitHub social preview", () => {
    expect(pngSize(read("site/public/og.png"))).toEqual([1200, 630]);
    expect(pngSize(read("site/public/media/github-social-preview.png"))).toEqual([1280, 640]);
  });

  it("shows the lockup in the header and shares the card", () => {
    const layout = read("site/src/layouts/Landing.astro").toString();
    expect(layout).toContain("packages/ui/assets/brand/rematch-icon.svg?raw");
    expect(layout).toContain('<span class="wordmark-name">rematch</span>');
    expect(layout).toContain('href("og.png")');
  });

  it("gives the docs the icon, ink by day and cream at night, never pen red", () => {
    const day = read("site/src/assets/rematch-icon-day.svg").toString();
    const night = read("site/src/assets/rematch-icon-night.svg").toString();
    expect(day).toContain('fill="#1F2A3D"');
    expect(night).toContain('fill="#E8E2D6"');
    for (const svg of [day, night]) expect(svg).not.toMatch(/#c8372d|#d3312b|currentColor/i);
  });
});
