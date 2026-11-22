export type TimeClass = "bullet" | "blitz" | "rapid" | "classical" | "daily";
export type PlayerResult = "win" | "draw" | "loss" | "unfinished";
export type AnalysisStatus = "pending" | "analysing" | "done" | "failed" | "skipped";
export type JudgementCounts = { blunders: number; mistakes: number; inaccuracies: number };
export type GameFilters = {
  timeClass: TimeClass | null;
  result: "win" | "draw" | "loss" | null;
  colour: "white" | "black" | null;
  from: string | null;
  to: string | null;
  accountId: string | null;
};
export type GameSortKey = "played" | "accuracy" | "opponent" | "blunders";
export type GameListQuery = {
  filters: GameFilters;
  /**
   * The time control the URL means when it names none: rapid when the player has rapid games
   * (I8). Then `tc=all` asks for every time control. Absent, no `tc` is every time control.
   */
  defaultTimeClass?: TimeClass;
  sort: { key: GameSortKey; dir: "asc" | "desc" };
  page: number;
};
export type GameRow = {
  id: string;
  accountId: string;
  platform: "lichess" | "chesscom";
  playedAt: Date;
  timeClass: TimeClass;
  baseSeconds: number | null;
  incrementSeconds: number | null;
  rated: boolean;
  playerColor: "white" | "black";
  player: { name: string; rating: number | null };
  opponent: { name: string; rating: number | null };
  result: PlayerResult;
  termination: string | null;
  eco: string | null;
  openingName: string | null;
  analysisStatus: AnalysisStatus;
  accuracy: number | null;
  counts: JudgementCounts | null;
  plies: number | null;
};
export type GamePage = { rows: GameRow[]; total: number; page: number; pageCount: number };
export const PAGE_SIZE = 25;
