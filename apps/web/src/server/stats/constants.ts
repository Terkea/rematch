/** Fewer analysed games than this and a page says "Need N more games" instead of a number. */
export const KPI_MIN_GAMES = 10;
export const TREND_WINDOW = 20;
/** The line starts on a full window, so "your average over the last 20 games" is literal (ADR-WEB-19). */
export const TREND_MIN = TREND_WINDOW;
export const CONFIDENCE = 0.8;
export const BOOTSTRAP_RESAMPLES = 2000;
export const TREND_WEEKS = 12;
export const CHART_GAMES = 500;
export const PER_GAME_COLUMNS = 40;
/** Two-sided normal quantiles. */
export const Z = { 0.8: 1.2815515655446004, 0.9: 1.6448536269514722 } as const;
/** One-sided 90% Student t quantiles (an 80% two-sided band) by degrees of freedom, 9 to 19. */
export const T80: Record<number, number> = {
  9: 1.383,
  10: 1.372,
  11: 1.363,
  12: 1.356,
  13: 1.35,
  14: 1.345,
  15: 1.341,
  16: 1.337,
  17: 1.333,
  18: 1.33,
  19: 1.328,
};
