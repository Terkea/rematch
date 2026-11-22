import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import LabLayout from "../src/app/lab/layout.js";
import LabIndex from "../src/app/lab/page.js";

// The review lab (Task 7) is dev-only: the layout guard keeps every /lab/review route behind
// notFound() in production, and the lab index lists the three variants.

const routes = ["/lab/review/a", "/lab/review/b", "/lab/review/c"] as const;

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("review lab guard", () => {
  for (const route of routes) {
    it(`${route} is not found in production`, () => {
      vi.stubEnv("NODE_ENV", "production");
      vi.stubEnv("LAB_ENABLED", "");
      expect(() => LabLayout({ children: route })).toThrow(/NEXT_HTTP_ERROR_FALLBACK;404/);
    });
  }

  it("renders in development", () => {
    vi.stubEnv("NODE_ENV", "development");
    expect(() => LabLayout({ children: "ok" })).not.toThrow();
  });
});

describe("lab index", () => {
  it("lists review with variants a, b and c", () => {
    vi.stubEnv("NODE_ENV", "development");
    const html = renderToStaticMarkup(createElement(LabIndex));
    expect(html).toContain('<h2 class="font-semibold">review</h2>');
    for (const route of routes) expect(html).toContain(`href="${route}"`);
  });
});
