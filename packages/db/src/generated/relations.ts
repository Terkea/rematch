import { relations } from "drizzle-orm/relations";
import { games, importRuns, linkedAccounts, moves } from "./schema.js";

export const gamesRelations = relations(games, ({ one, many }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [games.accountId],
    references: [linkedAccounts.id],
  }),
  moves: many(moves),
}));

export const linkedAccountsRelations = relations(linkedAccounts, ({ many }) => ({
  games: many(games),
  importRuns: many(importRuns),
}));

export const importRunsRelations = relations(importRuns, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [importRuns.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const movesRelations = relations(moves, ({ one }) => ({
  game: one(games, {
    fields: [moves.gameId],
    references: [games.id],
  }),
}));
