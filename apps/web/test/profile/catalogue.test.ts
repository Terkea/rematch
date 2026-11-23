import { readFileSync } from "node:fs";
import { areaLabels } from "@ci/ui/area-score";
import { describe, expect, it } from "vitest";
import {
  AREAS,
  contractId,
  KPIS,
  kpiDef,
  MOTIFS,
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
    expect(seriesLabel("motif.allowed:hangingPiece")).toBe("Allowed free pieces");
    expect(seriesLabel("motif.allowed:mateIn1")).toBe("Allowed mates in one");
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
    expect(seriesLabel("motif.missed:hangingPiece")).toBe("Missed free pieces");
    expect(seriesLabel("motif.allowed:fork")).toBe("Allowed forks");
    expect(seriesLabel("motif.allowed:backRankMate")).toBe("Allowed back-rank mates");
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
