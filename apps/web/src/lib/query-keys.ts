// TanStack Query keys, exactly as M4 Task 23 specifies them. M5a Task 12 lands this file first
// (LiveRefresh watches `games(accountId)`); T23 adds the stream that invalidates these keys.
export const queryKeys = {
  accounts: () => ["accounts"] as const,
  importProgress: (accountId: string) => ["import-progress", accountId] as const,
  games: (accountId: string) => ["games", accountId] as const,
  game: (gameId: string) => ["game", gameId] as const,
  activity: () => ["activity"] as const,
  // M5b: the coach surfaces. The stream invalidates these (`applyCoachEvent`).
  report: (accountId: string) => ["report", accountId] as const,
  weekly: (accountId: string) => ["weekly", accountId] as const,
  notes: (accountId: string) => ["notes", accountId] as const,
  thread: (threadId: string) => ["thread", threadId] as const,
  /** A streaming answer's chunks (`ChatStreamState`), filled by `chat.*` events. */
  chatStream: (messageId: string) => ["chat-stream", messageId] as const,
};
