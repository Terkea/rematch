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

  it("shows the mid-size mark and the wordmark in the header, the mark in the footer", () => {
    const layout = read("site/src/layouts/Landing.astro").toString();
    expect(layout).toContain("packages/ui/assets/brand/rematch-mark-mid.svg?raw");
    expect(layout).not.toContain("rematch-icon");
    expect(layout).toContain('<span class="wordmark-name">Rematch</span>');
    expect(layout.split('mark("wordmark-mark")')).toHaveLength(2);
    expect(layout.split('mark("footer-mark")')).toHaveLength(2);
    // The capital R is the wordmark: no stylesheet lowercases it.
    for (const css of ["site/src/styles/landing.css", "site/src/styles/docs.css"])
      expect(read(css).toString()).not.toMatch(/text-transform:\s*lowercase/);
    expect(layout).toContain('href("og.png")');
  });

  it("opens the landing hero with the full mark", () => {
    const index = read("site/src/pages/index.astro").toString();
    expect(index).toContain("packages/ui/assets/brand/rematch-mark.svg?raw");
    expect(index.match(/class="hero-mark"/g)).toHaveLength(1);
  });

  it("gives the docs header the mid mark, ink by day and cream at night, never pen red", () => {
    const config = read("site/astro.config.mjs").toString();
    expect(config).toContain('title: "Rematch"');
    expect(config).toContain("./src/assets/rematch-mark-day.svg");
    expect(config).toContain("./src/assets/rematch-mark-night.svg");
    const day = read("site/src/assets/rematch-mark-day.svg").toString();
    const night = read("site/src/assets/rematch-mark-night.svg").toString();
    expect(day).toContain('fill="#1F2A3D"');
    expect(night).toContain('fill="#E8E2D6"');
    for (const svg of [day, night]) expect(svg).not.toMatch(/#c8372d|#d3312b|currentColor/i);
  });
  it("carries the README's 'Why I'm building this' word for word, with the photo", () => {
    const readme = read("README.md").toString();
    const index = read("site/src/pages/index.astro").toString();
    const why = readme.match(/## Why I'm building this\n\n([^\n]+)\n/)?.[1];
    expect(why).toBeTruthy();
    const squash = (t: string) => t.replace(/\s+/g, " ");
    expect(squash(index)).toContain(squash(why ?? "-"));
    expect(index).toContain(
      'alt="My dad as a young boy in a striped shirt, holding a chess board under his arm, with his mum kneeling beside him."',
    );
    expect(read("site/public/media/dad.jpg").equals(read("docs/media/dad.jpg"))).toBe(true);
    // It comes after "Help build Rematch".
    expect(index.indexOf('id="why"')).toBeGreaterThan(index.indexOf('id="contribute"'));
    // The footer's note about the mark points at it.
    expect(read("site/src/layouts/Landing.astro").toString()).toContain('href("#why")');
  });
});
