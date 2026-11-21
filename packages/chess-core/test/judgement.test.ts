import { describe, expect, it } from "vitest";
import { judge } from "../src/judgement.js";

describe("judge", () => {
  it("uses winning chances drop thresholds", () => {
    expect(judge({ cp: 0 }, { cp: -200 })).toBe("blunder");
    expect(judge({ cp: 0 }, { cp: -120 })).toBe("mistake");
    expect(judge({ cp: 0 }, { cp: -60 })).toBe("inaccuracy");
    expect(judge({ cp: 0 }, { cp: -40 })).toBeNull();
  });

  it("applies the mate-created rules", () => {
    expect(judge({ cp: -50 }, { mate: -2 })).toBe("blunder");
    expect(judge({ cp: -800 }, { mate: -2 })).toBe("mistake");
    expect(judge({ cp: -1000 }, { mate: -2 })).toBe("inaccuracy");
  });

  it("applies the mate-lost rules", () => {
    expect(judge({ mate: 2 }, { cp: 50 })).toBe("blunder");
    expect(judge({ mate: 2 }, { cp: 800 })).toBe("mistake");
    expect(judge({ mate: 2 }, { cp: 1000 })).toBe("inaccuracy");
    expect(judge({ mate: 3 }, { mate: 2 })).toBeNull();
  });
});
