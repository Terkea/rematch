import { describe, expect, it } from "vitest";
import { labEnabled } from "../src/app/lab/lab-enabled.js";

describe("labEnabled", () => {
  it("is off in production", () => {
    expect(labEnabled({ NODE_ENV: "production" })).toBe(false);
  });
  it("is on in production only with LAB_ENABLED=1", () => {
    expect(labEnabled({ NODE_ENV: "production", LAB_ENABLED: "1" })).toBe(true);
    expect(labEnabled({ NODE_ENV: "production", LAB_ENABLED: "0" })).toBe(false);
  });
  it("is on in development and test", () => {
    expect(labEnabled({ NODE_ENV: "development" })).toBe(true);
    expect(labEnabled({})).toBe(true);
  });
});
