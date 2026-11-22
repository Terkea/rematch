import type { GameRow, JudgementCounts, PlayerResult, TimeClass } from "../games/types";
import type { Estimate } from "../stats";

export type Overview = {
  accounts: {
    id: string;
    platform: "lichess" | "chesscom";
    username: string;
    displayName: string;
    verified: boolean;
    lastSyncedAt: Date | null;
    lastImport: {
      status: "running" | "done" | "failed" | "paused";
      imported: number;
      finishedAt: Date | null;
    } | null;
  }[];
  status: { total: number; analysed: number; waiting: number; failed: number; skipped: number };
  timeClasses: { timeClass: TimeClass; analysed: number }[];
  accuracy: {
    points: { id: string; playedAt: Date; accuracy: number; result: PlayerResult }[];
    rolling: { id: string; mean: number; low: number | null; high: number | null }[];
    overall: Estimate;
  };
  judgements: {
    columns: ({ id: string; playedAt: Date } & JudgementCounts)[];
    blunders: Estimate;
    mistakes: Estimate;
    inaccuracies: Estimate;
    weekly: {
      week: string;
      blunders: number | null;
      mistakes: number | null;
      inaccuracies: number | null;
      accuracy: number | null;
      /** Analysed games behind the week's numbers. */
      games: number;
    }[];
  };
  byTimeClass: {
    timeClass: TimeClass;
    games: number;
    wins: number;
    draws: number;
    losses: number;
    score: Estimate;
    accuracy: Estimate;
  }[];
  recent: GameRow[];
};
