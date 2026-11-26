import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  ALGORITHM_ID,
  ALGORITHM_VERSION,
  algorithmLink,
  changelogAnchor,
} from "../src/server/algorithm";
import { FOCUS_VERSION } from "../src/server/focus/goal";
import { PHASE_METRICS_VERSION } from "../src/server/stats/game-stats";
import { TRAINER_VERSION } from "../src/server/train/model";

const root = join(__dirname, "../../..");
const manifest = JSON.parse(readFileSync(join(root, "algorithm/manifest.json"), "utf8"));
const digest = (rel: string) =>
  createHash("sha256")
    .update(readFileSync(join(root, rel), "utf8").replace(/\r\n/g, "\n"))
    .digest("hex");

// ADR 0105: the manifest's TS fields are the web's own constants and files. A failure means a
// trainer or focus rule changed: regenerate the manifest (`cargo run -p cli -- algo manifest
// --write`), bump the version and add an algorithm/CHANGELOG.md entry.
describe("the algorithm manifest's web fields", () => {
  it("names the trainer, focus and phase-metrics versions", () => {
    expect(manifest.components.recommendation.trainer_version).toBe(TRAINER_VERSION);
    expect(manifest.components.recommendation.focus_version).toBe(FOCUS_VERSION);
    expect(manifest.components.facts.phase_metrics_version).toBe(PHASE_METRICS_VERSION);
  });

  it("digests the trainer and focus rules as they are", () => {
    for (const rel of [
      "apps/web/src/server/train/model.ts",
      "apps/web/src/server/focus/goal.ts",
      "apps/web/src/server/profile/obvious-capture.ts",
    ]) {
      expect(manifest.components.recommendation.source_digest[rel], rel).toBe(digest(rel));
    }
  });

  it("is the id the web stamps", () => {
    expect(ALGORITHM_ID).toBe(manifest.id);
    expect(ALGORITHM_VERSION).toBe(manifest.version);
    expect(ALGORITHM_ID.startsWith(`algo-${manifest.version}+`)).toBe(true);
  });
});

describe("the changelog link", () => {
  it("uses GitHub's heading anchor", () => {
    expect(changelogAnchor("algo-1.0.0+4ca3a72b0566")).toBe("algo-1004ca3a72b0566");
    const link = algorithmLink("algo-1.0.0+4ca3a72b0566");
    expect(link.version).toBe("1.0.0");
    expect(link.href).toBe(
      "https://github.com/Terkea/rematch/blob/main/algorithm/CHANGELOG.md#algo-1004ca3a72b0566",
    );
  });

  it("reads an old row's missing id as before versioning", () => {
    expect(algorithmLink(null)).toEqual({ id: null, version: null, href: null });
  });
});
