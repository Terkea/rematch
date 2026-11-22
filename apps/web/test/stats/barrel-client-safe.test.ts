import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// The stats barrel is imported by client components (the overview view), so it must stay free
// of server-only modules: `game-stats.ts` pulls in `@ci/db` (node:crypto, node:fs) and broke the
// client bundle when it was re-exported here. Import it from "./game-stats" directly.
describe("server/stats barrel", () => {
  it("does not re-export the DB-backed game-stats module", () => {
    const barrel = readFileSync(new URL("../../src/server/stats/index.ts", import.meta.url), "utf8");
    expect(barrel).not.toMatch(/game-stats/);
  });
});
