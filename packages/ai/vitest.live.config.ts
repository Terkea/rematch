import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    name: "ai-live",
    include: ["test/live/**/*.test.ts"],
  },
});
