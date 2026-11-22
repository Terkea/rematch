import { describe, expect, it } from "vitest";
import { formatSseEvent, parseCursor } from "../../src/server/events/sse";

describe("formatSseEvent", () => {
  it("writes id, event and a JSON data line", () => {
    expect(
      formatSseEvent({
        id: 42,
        accountId: "a",
        type: "game.analysed",
        payload: { gameId: "g1", accuracy: 81.5 },
      }),
    ).toBe(
      'id: 42\nevent: game.analysed\ndata: {"accountId":"a","payload":{"gameId":"g1","accuracy":81.5}}\n\n',
    );
  });

  it("keeps a newline inside the payload on one data line", () => {
    const frame = formatSseEvent({ id: 1, accountId: "a", type: "t", payload: { s: "x\ny" } });
    expect(frame.split("\n").filter((l) => l.startsWith("data:"))).toHaveLength(1);
  });

  it("refuses an event type that could inject SSE fields", () => {
    expect(() =>
      formatSseEvent({ id: 1, accountId: "a", type: "t\ndata: x", payload: {} }),
    ).toThrow();
  });
});

describe("parseCursor", () => {
  it("accepts non-negative integers and refuses anything else", () => {
    expect(parseCursor(null)).toBeNull();
    expect(parseCursor("")).toBeNull();
    expect(parseCursor("0")).toBe(0);
    expect(parseCursor("123")).toBe(123);
    for (const bad of ["-1", "1.5", "1e3", " 1", "abc", "9".repeat(16)]) {
      expect(parseCursor(bad)).toBe("invalid");
    }
  });
});
