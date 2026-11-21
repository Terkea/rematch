import { relations } from "drizzle-orm/relations";
import {
  account,
  accountLinks,
  chesscomVerifications,
  games,
  importRuns,
  linkedAccounts,
  moves,
  session,
  user,
} from "./schema.js";

export const accountLinksRelations = relations(accountLinks, ({ one, many }) => ({
  chesscomVerifications: many(chesscomVerifications),
  linkedAccount: one(linkedAccounts, {
    fields: [accountLinks.linkedAccountId],
    references: [linkedAccounts.id],
  }),
  user: one(user, {
    fields: [accountLinks.userId],
    references: [user.id],
  }),
}));

export const accountRelations = relations(account, ({ one }) => ({
  user: one(user, {
    fields: [account.userId],
    references: [user.id],
  }),
}));

export const chesscomVerificationsRelations = relations(chesscomVerifications, ({ one }) => ({
  accountLink: one(accountLinks, {
    fields: [chesscomVerifications.accountLinkId],
    references: [accountLinks.id],
  }),
}));

export const gamesRelations = relations(games, ({ one, many }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [games.accountId],
    references: [linkedAccounts.id],
  }),
  moves: many(moves),
}));

export const importRunsRelations = relations(importRuns, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [importRuns.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const linkedAccountsRelations = relations(linkedAccounts, ({ many }) => ({
  accountLinks: many(accountLinks),
  games: many(games),
  importRuns: many(importRuns),
}));

export const movesRelations = relations(moves, ({ one }) => ({
  game: one(games, {
    fields: [moves.gameId],
    references: [games.id],
  }),
}));

export const sessionRelations = relations(session, ({ one }) => ({
  user: one(user, {
    fields: [session.userId],
    references: [user.id],
  }),
}));

export const userRelations = relations(user, ({ many }) => ({
  accountLinks: many(accountLinks),
  accounts: many(account),
  sessions: many(session),
}));
