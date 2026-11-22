import { afterEach } from "vitest";

// Required by src/env.ts; Compose passes an empty value unless .env sets one. Never a real address.

// Component tests opt in to jsdom with `// @vitest-environment jsdom`; register the DOM matchers only then.
if (typeof document !== "undefined") {
  await import("@testing-library/jest-dom/vitest");
  const { cleanup } = await import("@testing-library/react");
  afterEach(() => cleanup());
}
