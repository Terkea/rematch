import { relations } from "drizzle-orm/relations";
import {
  aiBatches,
  aiBatchItems,
  coachConversations,
  coachMessages,
  games,
  importRuns,
  interactiveEvals,
  linkedAccounts,
  moves,
  reportRuns,
  reportSections,
  reports,
  weaknessChangeNotes,
  weeklyReviews,
} from "./schema.js";

export const aiBatchItemsRelations = relations(aiBatchItems, ({ one }) => ({
  aiBatch: one(aiBatches, {
    fields: [aiBatchItems.batchId],
    references: [aiBatches.id],
  }),
  reportRun: one(reportRuns, {
    fields: [aiBatchItems.reportRunId],
    references: [reportRuns.id],
  }),
  weeklyReview: one(weeklyReviews, {
    fields: [aiBatchItems.weeklyReviewId],
    references: [weeklyReviews.id],
  }),
}));

export const aiBatchesRelations = relations(aiBatches, ({ many }) => ({
  aiBatchItems: many(aiBatchItems),
}));

export const coachConversationsRelations = relations(coachConversations, ({ one, many }) => ({
  coachMessages: many(coachMessages),
  linkedAccount: one(linkedAccounts, {
    fields: [coachConversations.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const coachMessagesRelations = relations(coachMessages, ({ one, many }) => ({
  coachConversation: one(coachConversations, {
    fields: [coachMessages.conversationId],
    references: [coachConversations.id],
  }),
  interactiveEvals: many(interactiveEvals),
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

export const interactiveEvalsRelations = relations(interactiveEvals, ({ one }) => ({
  coachMessage: one(coachMessages, {
    fields: [interactiveEvals.messageId],
    references: [coachMessages.id],
  }),
  linkedAccount: one(linkedAccounts, {
    fields: [interactiveEvals.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const linkedAccountsRelations = relations(linkedAccounts, ({ many }) => ({
  coachConversations: many(coachConversations),
  games: many(games),
  importRuns: many(importRuns),
  interactiveEvals: many(interactiveEvals),
  reportRuns: many(reportRuns),
  reports: many(reports),
  weaknessChangeNotes: many(weaknessChangeNotes),
  weeklyReviews: many(weeklyReviews),
}));

export const movesRelations = relations(moves, ({ one }) => ({
  game: one(games, {
    fields: [moves.gameId],
    references: [games.id],
  }),
}));

export const reportRunsRelations = relations(reportRuns, ({ one, many }) => ({
  aiBatchItems: many(aiBatchItems),
  linkedAccount: one(linkedAccounts, {
    fields: [reportRuns.accountId],
    references: [linkedAccounts.id],
  }),
  reports: many(reports),
}));

export const reportSectionsRelations = relations(reportSections, ({ one }) => ({
  report: one(reports, {
    fields: [reportSections.reportId],
    references: [reports.id],
  }),
}));

export const reportsRelations = relations(reports, ({ one, many }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [reports.accountId],
    references: [linkedAccounts.id],
  }),
  reportRun: one(reportRuns, {
    fields: [reports.reportRunId],
    references: [reportRuns.id],
  }),
  reportSections: many(reportSections),
}));

export const weaknessChangeNotesRelations = relations(weaknessChangeNotes, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [weaknessChangeNotes.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const weeklyReviewsRelations = relations(weeklyReviews, ({ one, many }) => ({
  aiBatchItems: many(aiBatchItems),
  linkedAccount: one(linkedAccounts, {
    fields: [weeklyReviews.accountId],
    references: [linkedAccounts.id],
  }),
}));
