import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ALGORITHM_ID } from "../src/lib/algorithm";
import { chooseTarget, focusProgress } from "../src/server/focus/goal";
import { LAB_STEPS, runLabStep } from "../src/server/lab/bridge";
import { isHeldOut, keepCandidate, primarySource, type SourceKey } from "../src/server/train/model";

// The lab's Node step (ADR 0115): each step on its committed request gives the committed golden
// file byte for byte (`lab/fixtures/golden/`, shared with the lab's own contract test), and each
// answer is exactly what the rule's exported function returns. `REPLAY_UPDATE=1` rewrites the
// golden files after a deliberate change of the rule (and an algorithm version, ADR 0105).

const lab = join(__dirname, "../../../lab");
const PLACEHOLDER = "ALGORITHM_ID";
const read = (rel: string) => readFileSync(join(lab, rel), "utf8").replace(/\r\n/g, "\n");
const request = (step: string) => read(`fixtures/${step}-request.json`);
const withPlaceholder = (out: string) =>
  out.replace(`"algorithm_id": "${ALGORITHM_ID}"`, `"algorithm_id": "${PLACEHOLDER}"`);

function golden(step: string, out: string) {
  const rel = `fixtures/golden/${step}-response.json`;
  if (process.env.REPLAY_UPDATE === "1") {
    writeFileSync(join(lab, rel), withPlaceholder(out));
    return;
  }
  expect(withPlaceholder(out), `${rel} differs from the bridge's output`).toBe(read(rel));
}

describe("the lab bridge's steps", () => {
  it.each(LAB_STEPS)("%s gives its golden file, twice the same bytes", (step) => {
    const out = runLabStep(step, request(step));
    expect(runLabStep(step, request(step))).toBe(out);
    const doc = JSON.parse(out);
    expect(doc.schema_version).toBe("lab-v1");
    expect(doc.algorithm_id).toBe(ALGORITHM_ID);
    golden(step, out);
  });

  it("focus-target is chooseTarget", () => {
    const req = JSON.parse(request("focus-target"));
    const doc = JSON.parse(runLabStep("focus-target", request("focus-target")));
    expect(doc.focus_version).toBe("focus-v1");
    req.items.forEach((i: { baseline: number; peer: number | null }, n: number) => {
      expect(doc.items[n].target).toEqual(chooseTarget(i));
    });
    expect(
      new Set(doc.items.map((i: { target: { basis: string } | null }) => i.target?.basis)),
    ).toEqual(new Set(["relative", "peer", undefined]));
  });

  it("focus-progress is focusProgress", () => {
    const req = JSON.parse(request("focus-progress"));
    const doc = JSON.parse(runLabStep("focus-progress", request("focus-progress")));
    req.items.forEach(
      (i: { k: number; n: number; games: number; games_n: number; target: number }, n: number) => {
        const direct = focusProgress({
          k: i.k,
          n: i.n,
          games: i.games,
          gamesN: i.games_n,
          target: i.target,
        });
        expect(doc.items[n].progress).toEqual(JSON.parse(JSON.stringify(direct)));
      },
    );
    const outcomes = doc.items.map(
      (i: { progress: { outcome: string | null } }) => i.progress.outcome,
    );
    expect(outcomes).toEqual(expect.arrayContaining(["met", "not_met", "inconclusive", null]));
  });

  it("trainer-sources is keepCandidate, primarySource and isHeldOut", () => {
    const req = JSON.parse(request("trainer-sources"));
    const doc = JSON.parse(runLabStep("trainer-sources", request("trainer-sources")));
    expect(doc.sources).toEqual(["W4.missed", "W3.shallow", "W3.deep", "W7.traps", "blunder"]);
    type C = {
      game_id: string;
      ply: number;
      fen: string;
      uci: string;
      best_uci: string | null;
      win_before: number;
      win_after: number;
      prev_san: string | null;
      sources: SourceKey[];
    };
    req.candidates.forEach((c: C, n: number) => {
      const kept = keepCandidate({
        gameId: c.game_id,
        ply: c.ply,
        fen: c.fen,
        uci: c.uci,
        bestUci: c.best_uci,
        winBefore: c.win_before,
        winAfter: c.win_after,
        prevSan: c.prev_san,
        sources: c.sources,
      });
      expect(doc.candidates[n]).toEqual({
        game_id: c.game_id,
        ply: c.ply,
        kept,
        primary_source: primarySource(c.sources),
        held_out: isHeldOut(c.game_id, c.ply),
      });
    });
    const by = (id: string) => doc.candidates.find((c: { game_id: string }) => c.game_id === id);
    expect(by("game-free-capture").kept).toBe(false);
    expect(by("game-free-capture-missed").kept).toBe(true);
    expect(by("game-kept").primary_source).toBe("W3.shallow");
    expect(doc.candidates.some((c: { held_out: boolean }) => c.held_out)).toBe(true);
  });

  it("refuses a bad request with the reason", () => {
    expect(() => runLabStep("focus-target", '{"schema_version":"lab-v0","items":[]}')).toThrow(
      /lab-v0/,
    );
    expect(() =>
      runLabStep("focus-target", '{"schema_version":"lab-v1","items":[{"id":"x","baseline":0.2}]}'),
    ).toThrow(/peer/);
    expect(() =>
      runLabStep(
        "trainer-sources",
        '{"schema_version":"lab-v1","candidates":[{"game_id":"g","ply":1,"fen":"","uci":"","best_uci":null,"win_before":0,"win_after":0,"prev_san":null,"sources":["W1"]}]}',
      ),
    ).toThrow(/sources/);
    expect(() => runLabStep("rank", "{}")).toThrow(/usage/);
    expect(() => runLabStep("focus-target", "not json")).toThrow(/not JSON/);
  });

  it("imports the rules and nothing that needs a database or Next.js", () => {
    const imports = (rel: string) =>
      [...readFileSync(join(__dirname, "..", rel), "utf8").matchAll(/from "([^"]+)"/g)].map(
        (m) => m[1],
      );
    expect(imports("src/server/lab/bridge.ts").sort()).toEqual(
      ["../../lib/algorithm", "../focus/goal", "../train/model", "zod"].sort(),
    );
    for (const rel of [
      "src/server/focus/goal.ts",
      "src/server/train/model.ts",
      "src/server/profile/obvious-capture.ts",
    ]) {
      for (const spec of imports(rel)) {
        expect(spec, `${rel} imports ${spec}`).not.toMatch(/@ci\/db|drizzle|next|postgres|ioredis/);
      }
    }
  });
});
