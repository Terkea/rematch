import { describe, expect, it } from "vitest";
import {
  buildEvidence,
  formatNumber,
  listNumberPaths,
  resolveDatePath,
  resolveNumberPath,
  resolveToken,
} from "../../src/validator/evidence.js";
import { parseTokens } from "../../src/validator/tokens.js";
import { loadFixture } from "../fixtures/make-input.js";

const input = loadFixture("b2-rapid");
const ev = buildEvidence(input);
const w1 = input.weaknesses[0];
const ex = w1?.exemplars[0];
const mk = (s: string) => {
  const t = parseTokens(s).tokens[0];
  if (!t) throw new Error("no token");
  return t;
};

describe("number path index", () => {
  it("resolves basis, weakness, metric and exemplar paths", () => {
    expect(resolveNumberPath(ev, "basis.analysedGames")).toBe(input.basis.analysedGames);
    expect(resolveNumberPath(ev, "basis.importedGames")).toBe(input.basis.importedGames);
    expect(resolveNumberPath(ev, "weaknesses.W1.posteriorRate")).toBe(w1?.posteriorRate);
    expect(resolveNumberPath(ev, "weaknesses.W1.events")).toBe(w1?.events);
    const m = input.metrics[0];
    expect(resolveNumberPath(ev, `metrics.${m?.key}.value`)).toBe(m?.value);
    expect(resolveNumberPath(ev, "timeControl.baseMinutes")).toBe(10);
    expect(resolveNumberPath(ev, "weaknesses.W1.exemplars.0.moveNumber")).toBe(
      Math.ceil((ex?.ply ?? 0) / 2),
    );
    const cp = ((ex?.evalAfter ?? {}) as { cp?: number }).cp ?? 0;
    expect(resolveNumberPath(ev, "weaknesses.W1.exemplars.0.evalAfter.pawns")).toBeCloseTo(
      cp / 100,
      9,
    );
    expect(resolveNumberPath(ev, "weaknesses.W1.exemplars.0.winAfter")).toBe(ex?.winAfter);
  });
  it("resolves changes by series key and counts", () => {
    const c = input.changes[0];
    expect(resolveNumberPath(ev, `changes.${c?.seriesKey.replace(/\./g, "_")}.rateAfter`)).toBe(
      c?.rateAfter,
    );
    expect(resolveNumberPath(ev, "weaknesses.count")).toBe(input.weaknesses.length);
  });
  it("returns undefined for unknown paths", () => {
    expect(resolveNumberPath(ev, "weaknesses.W99.posteriorRate")).toBeUndefined();
    expect(resolveNumberPath(ev, "nope")).toBeUndefined();
  });
  it("lists paths", () => {
    const paths = listNumberPaths(ev);
    expect(paths).toContain("basis.analysedGames");
    expect(paths).toContain("weaknesses.W1.posteriorRate");
    expect(new Set(paths).size).toBe(paths.length);
  });
  it("resolves date paths", () => {
    expect(resolveDatePath(ev, "basis.windowStart")).toBe(input.basis.windowStart);
    expect(resolveDatePath(ev, "nope")).toBeUndefined();
  });
});

describe("formatNumber", () => {
  it("formats every fmt", () => {
    expect(formatNumber(214.4, "int")).toBe("214");
    expect(formatNumber(0.3149, "pct0")).toBe("31%");
    expect(formatNumber(0.3149, "pct1")).toBe("31.5%");
    expect(formatNumber(1.234, "dec1")).toBe("1.2");
    expect(formatNumber(1.235, "dec2")).toBe("1.24");
    expect(formatNumber(1.5, "signed1")).toBe("+1.5");
    expect(formatNumber(-1.5, "signed1")).toBe("-1.5");
    expect(formatNumber(0, "signed1")).toBe("0.0");
    expect(formatNumber(3912, "int")).toBe("3,912");
  });
});

describe("resolveToken", () => {
  it("renders move, best and pv tokens", () => {
    if (!ex) throw new Error("fixture");
    const id = `${ex.gameId}:${ex.ply}`;
    expect(resolveToken(ev, mk(`{{move:${id}}}`))).toBe(ex.played.san);
    expect(resolveToken(ev, mk(`{{best:${id}}}`))).toBe(ex.best?.san);
    expect(resolveToken(ev, mk(`{{pv:${id}:best:2}}`))).toBe("dxe5");
    expect(resolveToken(ev, mk(`{{pv:${id}:refutation:2}}`))).toBe("Nxd4");
    expect(resolveToken(ev, mk(`{{pv:${id}:best:99}}`))).toBeUndefined();
    expect(resolveToken(ev, mk("{{move:nope:1}}"))).toBeUndefined();
  });
  it("renders line, num, date and sq tokens", () => {
    const o = input.openings[0];
    expect(resolveToken(ev, mk(`{{line:${o?.lineKey}}}`))).toBe(o?.movesSan.join(" "));
    expect(resolveToken(ev, mk(`{{line:${o?.lineKey}:2}}`))).toBe(o?.movesSan[1]);
    expect(resolveToken(ev, mk("{{num:basis.analysedGames|int}}"))).toBe("214");
    expect(resolveToken(ev, mk("{{date:basis.windowStart}}"))).toBe(input.basis.windowStart);
    expect(resolveToken(ev, mk("{{sq:f7}}"))).toBe("f7");
  });
});
