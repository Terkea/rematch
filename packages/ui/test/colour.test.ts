import { describe, expect, it } from "vitest";
import { contrast, over, parseColour } from "../src/lib";

describe("parseColour", () => {
  it.each([
    ["#16233B", [22, 35, 59, 1]],
    ["#fff", [255, 255, 255, 1]],
    ["#0008", [0, 0, 0, 136 / 255]],
    ["#16233B80", [22, 35, 59, 128 / 255]],
    ["rgb(22 35 59 / 0.16)", [22, 35, 59, 0.16]],
    ["rgb(22 35 59 / 16%)", [22, 35, 59, 0.16]],
    ["rgba(22, 35, 59, 0.5)", [22, 35, 59, 0.5]],
    ["rgb(22, 35, 59)", [22, 35, 59, 1]],
  ])("reads %s", (value, expected) => {
    expect(parseColour(value)).toEqual(expected);
  });

  it("returns undefined for anything else", () => {
    expect(parseColour("navy")).toBeUndefined();
    expect(parseColour("#12345")).toBeUndefined();
  });
});

describe("contrast", () => {
  it("is 21 for black on white and 1 for a colour on itself", () => {
    expect(contrast([0, 0, 0, 1], [255, 255, 255, 1])).toBeCloseTo(21, 5);
    expect(contrast([22, 35, 59, 1], [22, 35, 59, 1])).toBe(1);
  });

  it("composites a translucent colour first, rounded to 8 bits", () => {
    expect(over([0, 0, 0, 0.5], [255, 255, 255, 1])).toEqual([128, 128, 128, 1]);
  });
});
