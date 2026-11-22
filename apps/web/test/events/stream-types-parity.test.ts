import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { STREAM_TYPES } from "../../src/lib/realtime/use-event-stream.js";

// The browser's EventSource delivers only the types it listens for, so STREAM_TYPES must hold
// every type the worker stores (M5b T11: the R3 and P2 types were missing, and every coach event
// was dropped in SSE mode). Read from the Rust sources, so a new kind fails here first.

const rust = (path: string) =>
  readFileSync(new URL(`../../../../backend/crates/${path}`, import.meta.url), "utf8");

describe("STREAM_TYPES", () => {
  it("covers R1's UserEventType and R3's EVENT_KINDS", () => {
    const store = [...rust("events/src/store.rs").matchAll(/Self::\w+ => "([a-z_.]+)"/g)].map(
      (m) => m[1],
    );
    const ai = [
      ...rust("ai/src/events.rs").matchAll(/pub const [A-Z_]+: &str = "([a-z_.]+)";/g),
    ].map((m) => m[1]);
    expect(store.length).toBeGreaterThanOrEqual(5);
    expect(ai.length).toBeGreaterThanOrEqual(9);
    const listened: readonly string[] = STREAM_TYPES;
    for (const type of [...store, ...ai]) expect(listened, type).toContain(type);
  });
});
