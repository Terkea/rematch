import { defineConfig } from "vitest/config";

// Several test files share (and truncate) the worker test database, so files run one at a time.
export default defineConfig({ test: { fileParallelism: false } });
