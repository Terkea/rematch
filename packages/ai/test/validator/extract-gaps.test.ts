import { describe, expect, it } from "vitest";
import { extractMoves, extractNumbers } from "../../src/validator/extract.js";
import { normalizeText } from "../../src/validator/normalize.js";

describe("extractMoves gaps", () => {
  it("counts bare pawn moves adjacent to a SAN move or move number", () => {
    expect(extractMoves("12. Nf3 d5").san).toEqual(["Nf3", "d5"]);
    expect(extractMoves("1. e4 e5 2. Nf3 Nc6").san).toEqual(["e4", "e5", "Nf3", "Nc6"]);
    expect(extractMoves("Nxe5 d6 (e4)").san).toEqual(["Nxe5", "d6", "e4"]);
    expect(extractMoves("...d6").san).toEqual(["d6"]);
    expect(extractMoves("Nxe5 ...d6").san).toEqual(["Nxe5", "d6"]);
  });
  it("still ignores squares in prose", () => {
    expect(extractMoves("the knight on f3 guards e5 and d4").san).toEqual([]);
    expect(extractMoves("after Nf3 and then the pawn on e5").san).toEqual(["Nf3"]);
  });
  it("reads long algebraic as uci", () => {
    expect(extractMoves("e2-e4").uci).toEqual(["e2e4"]);
    expect(extractMoves("Ng1-f3").uci).toEqual(["g1f3"]);
    expect(extractMoves("e2xd5 and Qd1–h5").uci).toEqual(["e2d5", "d1h5"]);
  });
  it("reads promotion without equals sign", () => {
    expect(extractMoves("e8Q").san).toEqual(["e8=Q"]);
    expect(extractMoves("e8(Q)").san).toEqual(["e8=Q"]);
    expect(extractMoves("exd8N+").san).toEqual(["exd8=N"]);
  });
  it("reads castling with any dash variant", () => {
    for (const d of ["-", "–", "—", "‑", "−", "‐"]) {
      expect(extractMoves(`O${d}O`).san).toEqual(["O-O"]);
      expect(extractMoves(`0${d}0${d}0`).san).toEqual(["O-O-O"]);
      expect(extractMoves(`O${d}O${d}O`).san).toEqual(["O-O-O"]);
    }
  });
  it("strips the full invisible set inside moves", () => {
    for (const c of ["​", "‌", "‍", "﻿", "⁠", "­", "͏", "⁦", "⁩", "‪", "‮", "️", "︀", "ㅤ", "ᅟ", "឴", "឵"]) {
      expect(extractMoves(`N${c}f3`).san, JSON.stringify(c)).toEqual(["Nf3"]);
    }
  });
});

describe("extractNumbers gaps", () => {
  it("reads number words beyond twenty with singular and plural units", () => {
    expect(extractNumbers("thirty games")).toMatchObject([{ value: 30 }]);
    expect(extractNumbers("forty-two moves")).toMatchObject([{ value: 42 }]);
    expect(extractNumbers("twenty one blunders")).toMatchObject([{ value: 21 }]);
    expect(extractNumbers("one game")).toMatchObject([{ value: 1 }]);
    expect(extractNumbers("a hundred games")).toMatchObject([{ value: 100 }]);
    expect(extractNumbers("two hundred and five games")).toMatchObject([{ value: 205 }]);
    expect(extractNumbers("three openings")).toMatchObject([{ value: 3 }]);
    expect(extractNumbers("one of the games")).toEqual([]);
  });
  it("reads fractions as ratios", () => {
    expect(extractNumbers("a third of games")).toMatchObject([{ kind: "ratio" }]);
    expect(extractNumbers("a third of games")[0]?.value).toBeCloseTo(1 / 3, 9);
    expect(extractNumbers("a quarter of games")[0]?.value).toBeCloseTo(0.25, 9);
    expect(extractNumbers("two thirds")[0]?.value).toBeCloseTo(2 / 3, 9);
    expect(extractNumbers("half of your games")).toMatchObject([{ value: 0.5, kind: "ratio" }]);
  });
  it("only counts half in quantitative uses", () => {
    expect(extractNumbers("in the second half of the game")).toEqual([]);
    expect(extractNumbers("a half-open file")).toEqual([]);
    expect(extractNumbers("the first half")).toEqual([]);
  });
  it("types digit ratios", () => {
    expect(extractNumbers("2.5x")).toMatchObject([{ value: 2.5, kind: "ratio", decimals: 1 }]);
    expect(extractNumbers("3 times as often")).toMatchObject([{ value: 3, kind: "ratio" }]);
    expect(extractNumbers("2× more")).toMatchObject([{ value: 2, kind: "ratio" }]);
    expect(extractNumbers("31% of games")[0]?.kind).toBe("plain");
  });
  it("parses a leading-dot decimal", () => {
    expect(extractNumbers(".5%")).toMatchObject([{ value: 0.5, percent: true, decimals: 1 }]);
  });
  it("treats a percent range as a range, not a negative", () => {
    expect(extractNumbers("50%-60%").map((n) => n.value)).toEqual([50, 60]);
    expect(extractNumbers("50 %–60 %").map((n) => n.value)).toEqual([50, 60]);
  });
  it("flags comma decimals as ambiguous instead of splitting", () => {
    const r = extractNumbers("about 1,5 games");
    expect(r.length).toBe(1);
    expect(r[0]?.ambiguous).toBe(true);
    expect(extractNumbers("3,912 games")[0]?.ambiguous).toBe(false);
  });
  it("reads a space-grouped thousand (nbsp)", () => {
    expect(extractNumbers("1 000 games").map((n) => n.value)).toEqual([1000]);
    expect(extractNumbers("1 000 games").map((n) => n.value)).toEqual([1000]);
    expect(normalizeText("1 000")).toBe("1,000");
  });
});
