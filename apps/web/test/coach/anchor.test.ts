import { describe, expect, it } from "vitest";
import {
  type ChatAnchor,
  coachHref,
  decodeAnchor,
  encodeAnchor,
  fallbackTitle,
  parseAnchor,
} from "../../src/server/coach/anchor.js";

const REPORT = "0b7c6f5e-3d1a-4c2b-9e8f-7a6b5c4d3e2f";

const anchors: ChatAnchor[] = [
  { kind: "report_section", reportId: REPORT, section: "climb_plan" },
  { kind: "weakness", weaknessId: "W3" },
  { kind: "position", gameId: "g-1", ply: 12 },
  { kind: "move", gameId: "g-2", ply: 1 },
  { kind: "opening_line", lineKey: "e4 e5 Nf3 Nc6" },
  { kind: "game", gameId: "g-3" },
];

describe("anchors", () => {
  it.each(anchors)("round trips $kind", (a) => {
    const s = encodeAnchor(a);
    expect(s).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(decodeAnchor(s)).toEqual(a);
    expect(parseAnchor(JSON.parse(JSON.stringify(a)))).toEqual(a);
  });

  it("parseAnchor rejects bad input", () => {
    for (const bad of [
      { kind: "weakness" },
      { kind: "move", gameId: "g", ply: -1 },
      { kind: "move", gameId: "g", ply: 0 },
      { kind: "move", gameId: "g", ply: 1001 },
      { kind: "move", gameId: "g", ply: 1.5 },
      { kind: "position", gameId: "g", ply: "5" },
      { kind: "report_section", reportId: "x", section: "overview" },
      { kind: "report_section", reportId: REPORT, section: "kingside" },
      { kind: "weakness", weaknessId: "W3", extra: 1 },
      { kind: "game", gameId: "" },
      { kind: "game", gameId: "a\u0000b" },
      { kind: "game", gameId: "x".repeat(201) },
      { kind: "board", gameId: "g" },
      "weakness",
      null,
      [],
      42,
    ]) {
      expect(parseAnchor(bad)).toBeNull();
    }
  });

  it("decodeAnchor drops garbage", () => {
    expect(decodeAnchor("")).toBeNull();
    expect(decodeAnchor("not base64!")).toBeNull();
    expect(decodeAnchor(Buffer.from("{oops").toString("base64url"))).toBeNull();
    expect(decodeAnchor(Buffer.from('{"kind":"weakness"}').toString("base64url"))).toBeNull();
    expect(decodeAnchor("A".repeat(5000))).toBeNull();
  });

  it("coachHref links to a new chat in the scope", () => {
    const a: ChatAnchor = { kind: "weakness", weaknessId: "W3" };
    expect(coachHref(a, { accountId: "acc-1", timeClass: "rapid" })).toBe(
      `/app/coach/new?account=acc-1&tc=rapid&anchor=${encodeAnchor(a)}`,
    );
    expect(coachHref(null, { accountId: "acc-1", timeClass: "blitz" })).toBe(
      "/app/coach/new?account=acc-1&tc=blitz",
    );
  });

  it("fallbackTitle follows the anchor", () => {
    expect(fallbackTitle(null)).toBe("Chat with your coach");
    expect(fallbackTitle({ kind: "game", gameId: "g" })).toBe("Chat about a game");
  });
});
