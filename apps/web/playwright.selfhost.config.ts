import { defineConfig, devices } from "@playwright/test";

// The self-host smoke test (ADR 0097, .github/workflows/self-host.yml): one spec against the dev
// stack that `scripts/setup.sh minimal` started and `pnpm seed:demo` filled. Unlike the e2e
// config it has no globalSetup: it must never empty the dev database. Run it in the qa container
// with docker-compose.smoke.yml (that file has the command).
export default defineConfig({
  testDir: "./e2e-selfhost",
  outputDir: "./test-results/selfhost",
  workers: 1,
  timeout: 600_000,
  expect: { timeout: 60_000 },
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    baseURL: process.env.E2E_BASE_URL ?? "http://localhost:3100",
    navigationTimeout: 180_000,
  },
});
