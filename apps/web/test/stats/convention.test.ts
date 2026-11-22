import { INTERVAL_LEVEL, KPI_MIN_GAMES } from "@ci/ui/stats";
import { describe, expect, it } from "vitest";
import {
  CONFIDENCE,
  KPI_MIN_GAMES as SERVER_MIN,
  TREND_MIN,
  TREND_WINDOW,
} from "../../src/server/stats/constants";

// One convention across the server's estimates and the UI's KPI rows (web polish critic I7,
// ADR-WEB-19): the same minimum games before any headline number, and the same interval level.
describe("the statistics convention", () => {
  it("uses one minimum and one interval level on the server and in the UI", () => {
    expect(SERVER_MIN).toBe(KPI_MIN_GAMES);
    expect(CONFIDENCE).toBe(INTERVAL_LEVEL);
  });

  it("draws the rolling line only over a full window, so 'the last 20 games' is literal", () => {
    expect(TREND_MIN).toBe(TREND_WINDOW);
    expect(TREND_WINDOW).toBe(20);
  });
});
