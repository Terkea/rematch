import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import LabLayout from "../src/app/lab/layout.js";
import PageA from "../src/app/lab/overview/a/page.js";
import PageB from "../src/app/lab/overview/b/page.js";
import PageC from "../src/app/lab/overview/c/page.js";
import LabIndex from "../src/app/lab/page.js";

// The pages load the signed-in user's overview; the guard must refuse before any of that runs.
const load = vi.hoisted(() => ({ loadLabOverview: vi.fn(), loadLabOverviewByClass: vi.fn() }));
vi.mock("../src/app/lab/overview/load", () => load);

const NOT_FOUND = "NEXT_HTTP_ERROR_FALLBACK;404";
vi.mock("next/navigation", () => ({
  notFound: () => {
    throw new Error(NOT_FOUND);
  },
}));

// Imported before NODE_ENV is stubbed, so React's dev JSX runtime is the one loaded.
const variants = ["a", "b", "c"] as const;
const pages = { a: PageA, b: PageB, c: PageC };

describe("overview lab guard", () => {
  beforeEach(() => {
    vi.stubEnv("NODE_ENV", "production");
    // Compose turns the lab on for development (LAB_ENABLED=1); a production build has it unset.
    vi.stubEnv("LAB_ENABLED", undefined);
  });
  afterEach(() => {
    vi.unstubAllEnvs();
    load.loadLabOverview.mockReset();
    load.loadLabOverviewByClass.mockReset();
  });

  it("keeps the lab layout behind notFound() in production", () => {
    expect(() => LabLayout({ children: null })).toThrow(NOT_FOUND);
  });

  for (const v of variants) {
    it(`keeps /lab/overview/${v} behind notFound() in production, before loading any data`, async () => {
      await expect(pages[v]()).rejects.toThrow(NOT_FOUND);
      expect(load.loadLabOverview).not.toHaveBeenCalled();
      expect(load.loadLabOverviewByClass).not.toHaveBeenCalled();
    });
  }
});

describe("lab index", () => {
  it("lists overview with variants a, b and c", () => {
    const html = renderToStaticMarkup(LabIndex());
    expect(html).toContain(">overview</h2>");
    for (const v of variants) expect(html).toContain(`href="/lab/overview/${v}"`);
  });
});
