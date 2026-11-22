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
  // The compose web service is `next dev`, which compiles a route on its first request.
  expect: { timeout: 15_000 },
  reporter: [["list"], ["html", { outputFolder: "playwright-report", open: "never" }]],
  use: { baseURL: process.env.E2E_BASE_URL ?? "http://localhost:3100" },
  projects: [
    { name: "e2e", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
    visual(390, 844, "light"),
    visual(390, 844, "dark"),
    visual(1440, 900, "light"),
    visual(1440, 900, "dark"),
  ],
});
