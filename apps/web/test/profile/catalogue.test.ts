import { readFileSync } from "node:fs";
import { areaLabels } from "@ci/ui/area-score";
import { describe, expect, it } from "vitest";
import {
  AREAS,
  contractId,
  isPlayerVisibleSeries,
  isPlayerVisibleTag,
  KPIS,
  kpiDef,
  MOTIFS,
  PLAYER_VISIBLE_MOTIFS,
  seriesArea,
  seriesLabel,
  WEAKNESSES,
  weaknessDef,
} from "../../src/server/profile/catalogue";
import { KPI_GLOSSARY, WEAKNESS_GLOSSARY } from "../../src/server/profile/glossary";

const fixture = JSON.parse(
  readFileSync(
    new URL("../../../../backend/fixtures/profile/catalogue.json", import.meta.url),
    "utf8",
  ),
) as { weaknesses: unknown[]; motifs: unknown[]; kpis: unknown[] };

describe("profile catalogue", () => {
  it("TS catalogue equals the Rust snapshot", () => {
    expect(WEAKNESSES).toEqual(fixture.weaknesses);
    expect(KPIS).toEqual(fixture.kpis);
    expect(MOTIFS).toEqual(fixture.motifs);
  });

  // One plain name per weakness and KPI (audit A2): the rows, details, popovers, report, Strengths
  // and the coach all say the glossary's name, so the catalogue (and Rust's) must carry it too.
  it("names every weakness and KPI with its glossary name", () => {
    for (const w of WEAKNESSES) expect(w.label, w.key).toBe(WEAKNESS_GLOSSARY[w.key]?.name);
    for (const k of KPIS) expect(k.label, k.metric).toBe(KPI_GLOSSARY[k.metric]?.name);
  });

  it("names an allowed motif after its missed twin", () => {
    expect(seriesLabel("motif.allowed:fork")).toBe("Allowed forks");
    expect(seriesLabel("motif.allowed:mateIn1")).toBe("Allowed mates in one");
  });

  // EXP-0007 (ADR 0112): six tags are computed but never named to a player.
  it("names only the motifs EXP-0007 cleared", () => {
    const hidden = ["hangingPiece", "pin", "skewer", "deflection", "sacrifice", "backRankMate"];
    const kpiMotifs = KPIS.filter((k) => k.metric.startsWith("motif_miss_rate:")).map((k) =>
      k.metric.slice("motif_miss_rate:".length),
    );
    expect(new Set(kpiMotifs)).toEqual(PLAYER_VISIBLE_MOTIFS);
    expect(
      MOTIFS.filter((m) => !PLAYER_VISIBLE_MOTIFS.has(m.tag))
        .map((m) => m.tag)
        .sort(),
    ).toEqual([...hidden].sort());
    for (const t of hidden) {
      expect(kpiDef(`motif_miss_rate:${t}`), t).toBeNull();
      expect(isPlayerVisibleSeries(`motif.missed:${t}`), t).toBe(false);
      expect(isPlayerVisibleSeries(`motif.allowed:${t}`), t).toBe(false);
    }
    expect(isPlayerVisibleSeries("motif.missed:fork")).toBe(true);
    expect(isPlayerVisibleSeries("W1")).toBe(true);
    expect(isPlayerVisibleSeries("opening:white:italian-game")).toBe(true);
  });

  it("falls back to the generic tactic name for a hidden motif, as Rust does", () => {
    expect(seriesLabel("motif.missed:pin")).toBe("Missed tactics");
    expect(seriesLabel("motif.allowed:hangingPiece")).toBe("Allowed tactics");
    expect(seriesLabel("motif.missed:nope")).toBe("motif.missed:nope");
  });

  it("keeps an exemplar's visible tags and any tag that is not a motif", () => {
    expect(isPlayerVisibleTag("fork")).toBe(true);
    expect(isPlayerVisibleTag("mate_in_4_plus")).toBe(true);
    expect(isPlayerVisibleTag("hung_piece")).toBe(true);
    expect(isPlayerVisibleTag("hanging_piece")).toBe(false);
    expect(isPlayerVisibleTag("back_rank_mate")).toBe(false);
  });

  it("areas follow the UI's order", () => {
    expect(AREAS).toEqual(Object.keys(areaLabels));
  });

  it("looks definitions up by key", () => {
    expect(weaknessDef("W1")?.minEvidence).toBe(1000);
    expect(weaknessDef("W99")).toBeNull();
    expect(kpiDef("motif_miss_rate:fork")?.label).toBe("Missed forks");
    expect(kpiDef("area:openings")).toBeNull();
  });

  it("labels every series kind", () => {
    expect(seriesLabel("W1")).toBe("Leaving pieces hanging");
    expect(seriesLabel("W3.shallow")).toBe("Missing a short tactic");
    expect(seriesLabel("motif.missed:fork")).toBe("Missed forks");
    expect(seriesLabel("motif.missed:smotheredMate")).toBe("Missed smothered mates");
    expect(seriesLabel("motif.allowed:fork")).toBe("Allowed forks");
    expect(seriesLabel("motif.allowed:mateIn4Plus")).toBe("Allowed long mates");
    expect(seriesLabel("opening:black:caro-kann-defence", "Caro-Kann Defence")).toBe(
      "Caro-Kann Defence (as Black)",
    );
    expect(seriesLabel("opening:white:italian-game")).toBe("Italian game (as White)");
    expect(seriesLabel("opening:white:italian-game", null)).toBe("Italian game (as White)");
    expect(seriesLabel("endgame:rook")).toBe("Rook endgames");
    expect(seriesLabel("endgame:minor")).toBe("Minor-piece endgames");
  });

  it("maps series to areas and contract ids", () => {
    expect(seriesArea("W9b")).toBe("time");
    expect(seriesArea("W1")).toBe("calculation");
    expect(seriesArea("motif.missed:pin")).toBe("tactics");
    expect(seriesArea("motif.allowed:pin")).toBe("tactics");
    expect(seriesArea("opening:white:italian-game")).toBe("openings");
    expect(seriesArea("endgame:pawn")).toBe("endgame");
    expect(contractId("W3.shallow")).toBe("W3");
    expect(contractId("W9a")).toBe("W9a");
    expect(contractId("motif.missed:pin")).toBeNull();
    expect(contractId("endgame:rook")).toBeNull();
  });
});
