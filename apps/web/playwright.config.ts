import { defineConfig, devices } from "@playwright/test";

const visual = (width: 390 | 1440, height: number, colorScheme: "light" | "dark") => ({
  name: `visual-${width}-${colorScheme}`,
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width, height },
    colorScheme,
    reducedMotion: "reduce" as const,
  },
});

export default defineConfig({
  testDir: "./e2e",
  outputDir: "./test-results",
  // Migrates and empties the e2e database, empties Mailpit and resets the fakes.
  globalSetup: "./e2e/global-setup.ts",
  // The specs share one database and the app's sign-in rate limits, so they run one at a time.
  workers: 1,
  fullyParallel: false,
  // The compose web service is `next dev`, which compiles a route (and its API) on first use and
  // can answer slowly on a loaded host. Until e2e runs on a production build (T24/T28), the
  // server-bound steps get generous time.
  timeout: 240_000,
  expect: { timeout: 30_000 },
  reporter: [["list"], ["html", { outputFolder: "playwright-report", open: "never" }]],
  use: {
    baseURL: process.env.E2E_BASE_URL ?? "http://localhost:3100",
    navigationTimeout: 120_000,
  },
  projects: [
    { name: "e2e", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
    visual(390, 844, "light"),
    visual(390, 844, "dark"),
    visual(1440, 900, "light"),
    visual(1440, 900, "dark"),
  ],
});
