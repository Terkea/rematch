/** Fewer analysed games than this and a page says "Need N more games" instead of a number. */
export const KPI_MIN_GAMES = 10;
export const TREND_WINDOW = 20;
export const TREND_MIN = 10;
export const CONFIDENCE = 0.8;
export const BOOTSTRAP_RESAMPLES = 2000;
export const TREND_WEEKS = 12;
export const CHART_GAMES = 500;
export const PER_GAME_COLUMNS = 40;
/** Two-sided normal quantiles. */
export const Z = { 0.8: 1.2815515655446004, 0.9: 1.6448536269514722 } as const;
/** A band is never narrower than this on each side (identical values say nothing about spread). */
export const MIN_HALF_WIDTH = 0.05;
