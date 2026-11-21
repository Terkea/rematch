import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Test files share one Postgres database, so they must not run in parallel.
    fileParallelism: false,
    setupFiles: ["./test/setup.ts"],
  },
});
