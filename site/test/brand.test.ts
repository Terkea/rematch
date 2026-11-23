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

  it("shows the wordmark alone in the header and shares the card", () => {
    const layout = read("site/src/layouts/Landing.astro").toString();
    expect(layout).not.toContain("rematch-icon");
    expect(layout).toContain('<span class="wordmark-name">Rematch</span>');
    // The capital R is the wordmark: no stylesheet lowercases it.
    for (const css of ["site/src/styles/landing.css", "site/src/styles/docs.css"])
      expect(read(css).toString()).not.toMatch(/text-transform:\s*lowercase/);
    expect(layout).toContain('href("og.png")');
  });

  it("gives the docs the wordmark alone, no icon in the header", () => {
    const config = read("site/astro.config.mjs").toString();
    expect(config).toContain('title: "Rematch"');
    expect(config).not.toMatch(/logo:\s*\{/);
  });
});
