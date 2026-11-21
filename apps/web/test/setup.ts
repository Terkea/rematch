import { afterEach } from "vitest";

// Component tests opt in to jsdom with `// @vitest-environment jsdom`; register the DOM matchers only then.
if (typeof document !== "undefined") {
  await import("@testing-library/jest-dom/vitest");
  const { cleanup } = await import("@testing-library/react");
  afterEach(() => cleanup());
}
