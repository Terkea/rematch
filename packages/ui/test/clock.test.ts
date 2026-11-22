import { describe, expect, it } from "vitest";
import { clockDuration, formatClock, formatSpent } from "../src/review/index.js";

describe("formatClock", () => {
  it("shows tenths below 20 s", () => {
    expect(formatClock(8.43)).toBe("0:08.4");
    expect(formatClock(0)).toBe("0:00.0");
  });

  it("floors, so it never reads more time than remains", () => {
    expect(formatClock(19.96)).toBe("0:19.9");
    expect(formatClock(9.99)).toBe("0:09.9");
  });

  it("switches to m:ss at 20 s", () => {
    expect(formatClock(20)).toBe("0:20");
    expect(formatClock(59.9)).toBe("0:59");
  });

  it("shows m:ss below an hour", () => {
    expect(formatClock(545)).toBe("9:05");
    expect(formatClock(3599.9)).toBe("59:59");
  });

  it("shows h:mm:ss below a day", () => {
    expect(formatClock(3723)).toBe("1:02:03");
    expect(formatClock(3600)).toBe("1:00:00");
  });

  it("shows days and hours from a day up", () => {
    expect(formatClock(183600)).toBe("2 d 3 h");
    expect(formatClock(86400)).toBe("1 d 0 h");
  });

  it("clamps negative input to 0", () => {
    expect(formatClock(-5)).toBe("0:00.0");
  });

  it("gives an empty string for a value that is not a finite number", () => {
    expect(formatClock(Number.NaN)).toBe("");
    expect(formatClock(Number.POSITIVE_INFINITY)).toBe("");
  });
});

describe("clockDuration", () => {
  it("is an ISO 8601 duration for <time dateTime>", () => {
    expect(clockDuration(545)).toBe("PT9M5S");
    expect(clockDuration(8.43)).toBe("PT8.4S");
    expect(clockDuration(183600)).toBe("PT51H");
  });
});

describe("formatSpent", () => {
  it("reads one decimal of seconds below a minute", () => {
    expect(formatSpent(12.43)).toBe("12.4 s on this move");
  });

  it("uses the clock format from a minute up", () => {
    expect(formatSpent(65)).toBe("1:05 on this move");
  });

  it("is null for a value it can't honestly show", () => {
    expect(formatSpent(Number.NaN)).toBeNull();
    expect(formatSpent(-1)).toBeNull();
  });
});

describe("whole-second sources", () => {
  it("formatClock drops the tenths when asked", () => {
    expect(formatClock(8.43, { tenths: false })).toBe("0:08");
    expect(formatClock(0, { tenths: false })).toBe("0:00");
    expect(formatClock(545, { tenths: false })).toBe("9:05");
  });

  it("formatSpent reads whole seconds when asked", () => {
    expect(formatSpent(12.43, { tenths: false })).toBe("12 s on this move");
  });
});
