import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Server tests (database, redis) share chess_web_test, so files run one at a time.
export default defineConfig({
  plugins: [react()],
  test: {
    fileParallelism: false,
    setupFiles: ["./test/setup.ts"],
    include: ["test/**/*.test.{ts,tsx}"],
  },
});
