import { relations } from "drizzle-orm/relations";
import {
  account,
  accountBenchmark,
  accountBenchmarkStat,
  accountLinks,
  aiBatches,
  aiBatchItems,
  bandReference,
  chesscomRequestBudgets,
  chesscomVerifications,
  coachConversations,
  coachingInputs,
  coachMessages,
  drillAttempts,
  drillItems,
  focuses,
  gameMetrics,
  gameOpenings,
  gamePreviews,
  games,
  importRuns,
  interactiveEvals,
  kpiSnapshot,
  linkedAccounts,
  moveFeatures,
  moves,
  openingExplorer,
  openingLines,
  opponentGameMetrics,
  referenceBuilds,
  referenceGames,
  referenceObservations,
  repertoireNodes,
  reportRuns,
  reportSections,
  reports,
  session,
  user,
  weaknessChangeNotes,
  weaknessEstimates,
  weaknessSeriesPoint,
  weaknessStateHistory,
  weeklyReviews,
} from "./schema.js";

export const accountBenchmarkRelations = relations(accountBenchmark, ({ one, many }) => ({
  accountBenchmarkStats: many(accountBenchmarkStat),
  linkedAccount: one(linkedAccounts, {
    fields: [accountBenchmark.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const accountBenchmarkStatRelations = relations(accountBenchmarkStat, ({ one }) => ({
  accountBenchmark: one(accountBenchmark, {
    fields: [accountBenchmarkStat.accountId],
    references: [accountBenchmark.accountId],
  }),
}));

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

export const bandReferenceRelations = relations(bandReference, ({ one }) => ({
  referenceBuild: one(referenceBuilds, {
    fields: [bandReference.buildId],
    references: [referenceBuilds.id],
  }),
}));

export const chesscomRequestBudgetsRelations = relations(chesscomRequestBudgets, ({ one }) => ({
  user: one(user, {
    fields: [chesscomRequestBudgets.userId],
    references: [user.id],
  }),
}));

export const chesscomVerificationsRelations = relations(chesscomVerifications, ({ one }) => ({
  accountLink: one(accountLinks, {
    fields: [chesscomVerifications.accountLinkId],
    references: [accountLinks.id],
  }),
}));

export const coachConversationsRelations = relations(coachConversations, ({ one, many }) => ({
  coachMessages: many(coachMessages),
  linkedAccount: one(linkedAccounts, {
    fields: [coachConversations.accountId],
    references: [linkedAccounts.id],
  }),
  user: one(user, {
    fields: [coachConversations.userId],
    references: [user.id],
  }),
}));

export const coachMessagesRelations = relations(coachMessages, ({ one, many }) => ({
  coachConversation: one(coachConversations, {
    fields: [coachMessages.conversationId],
    references: [coachConversations.id],
  }),
  coachingInput: one(coachingInputs, {
    fields: [coachMessages.coachingInputId],
    references: [coachingInputs.id],
  }),
  interactiveEvals: many(interactiveEvals),
}));

export const coachingInputsRelations = relations(coachingInputs, ({ one, many }) => ({
  coachMessages: many(coachMessages),
  linkedAccount: one(linkedAccounts, {
    fields: [coachingInputs.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const drillAttemptsRelations = relations(drillAttempts, ({ one }) => ({
  drillItem: one(drillItems, {
    fields: [drillAttempts.itemId],
    references: [drillItems.id],
  }),
  user: one(user, {
    fields: [drillAttempts.userId],
    references: [user.id],
  }),
}));

export const drillItemsRelations = relations(drillItems, ({ one, many }) => ({
  drillAttempts: many(drillAttempts),
  game: one(games, {
    fields: [drillItems.gameId],
    references: [games.id],
  }),
  linkedAccount: one(linkedAccounts, {
    fields: [drillItems.accountId],
    references: [linkedAccounts.id],
  }),
  user: one(user, {
    fields: [drillItems.userId],
    references: [user.id],
  }),
}));

export const focusesRelations = relations(focuses, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [focuses.accountId],
    references: [linkedAccounts.id],
  }),
  user: one(user, {
    fields: [focuses.userId],
    references: [user.id],
  }),
}));

export const gameMetricsRelations = relations(gameMetrics, ({ one }) => ({
  game: one(games, {
    fields: [gameMetrics.gameId],
    references: [games.id],
  }),
  linkedAccount: one(linkedAccounts, {
    fields: [gameMetrics.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const gameOpeningsRelations = relations(gameOpenings, ({ one }) => ({
  game: one(games, {
    fields: [gameOpenings.gameId],
    references: [games.id],
  }),
  linkedAccount: one(linkedAccounts, {
    fields: [gameOpenings.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const gamePreviewsRelations = relations(gamePreviews, ({ one }) => ({
  game: one(games, {
    fields: [gamePreviews.gameId],
    references: [games.id],
  }),
}));

export const gamesRelations = relations(games, ({ one, many }) => ({
  drillItems: many(drillItems),
  gameMetrics: many(gameMetrics),
  gameOpenings: many(gameOpenings),
  gamePreviews: many(gamePreviews),
  linkedAccount: one(linkedAccounts, {
    fields: [games.accountId],
    references: [linkedAccounts.id],
  }),
  moveFeatures: many(moveFeatures),
  moves: many(moves),
  opponentGameMetrics: many(opponentGameMetrics),
  weaknessSeriesPoints: many(weaknessSeriesPoint),
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

export const kpiSnapshotRelations = relations(kpiSnapshot, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [kpiSnapshot.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const linkedAccountsRelations = relations(linkedAccounts, ({ many }) => ({
  accountBenchmarks: many(accountBenchmark),
  accountLinks: many(accountLinks),
  coachConversations: many(coachConversations),
  coachingInputs: many(coachingInputs),
  drillItems: many(drillItems),
  focuses: many(focuses),
  gameMetrics: many(gameMetrics),
  gameOpenings: many(gameOpenings),
  games: many(games),
  importRuns: many(importRuns),
  interactiveEvals: many(interactiveEvals),
  kpiSnapshots: many(kpiSnapshot),
  openingLines: many(openingLines),
  opponentGameMetrics: many(opponentGameMetrics),
  repertoireNodes: many(repertoireNodes),
  reportRuns: many(reportRuns),
  reports: many(reports),
  weaknessChangeNotes: many(weaknessChangeNotes),
  weaknessEstimates: many(weaknessEstimates),
  weaknessSeriesPoints: many(weaknessSeriesPoint),
  weaknessStateHistories: many(weaknessStateHistory),
  weeklyReviews: many(weeklyReviews),
}));

export const moveFeaturesRelations = relations(moveFeatures, ({ one }) => ({
  game: one(games, {
    fields: [moveFeatures.gameId],
    references: [games.id],
  }),
}));

export const movesRelations = relations(moves, ({ one }) => ({
  game: one(games, {
    fields: [moves.gameId],
    references: [games.id],
  }),
}));

export const openingExplorerRelations = relations(openingExplorer, ({ one }) => ({
  referenceBuild: one(referenceBuilds, {
    fields: [openingExplorer.buildId],
    references: [referenceBuilds.id],
  }),
}));

export const openingLinesRelations = relations(openingLines, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [openingLines.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const opponentGameMetricsRelations = relations(opponentGameMetrics, ({ one }) => ({
  game: one(games, {
    fields: [opponentGameMetrics.gameId],
    references: [games.id],
  }),
  linkedAccount: one(linkedAccounts, {
    fields: [opponentGameMetrics.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const referenceBuildsRelations = relations(referenceBuilds, ({ many }) => ({
  bandReferences: many(bandReference),
  openingExplorers: many(openingExplorer),
  referenceGames: many(referenceGames),
  referenceObservations: many(referenceObservations),
}));

export const referenceGamesRelations = relations(referenceGames, ({ one, many }) => ({
  referenceBuild: one(referenceBuilds, {
    fields: [referenceGames.buildId],
    references: [referenceBuilds.id],
  }),
  referenceObservations: many(referenceObservations),
}));

export const referenceObservationsRelations = relations(referenceObservations, ({ one }) => ({
  referenceBuild: one(referenceBuilds, {
    fields: [referenceObservations.buildId],
    references: [referenceBuilds.id],
  }),
  referenceGame: one(referenceGames, {
    fields: [referenceObservations.referenceGameId],
    references: [referenceGames.id],
  }),
}));

export const repertoireNodesRelations = relations(repertoireNodes, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [repertoireNodes.accountId],
    references: [linkedAccounts.id],
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

export const sessionRelations = relations(session, ({ one }) => ({
  user: one(user, {
    fields: [session.userId],
    references: [user.id],
  }),
}));

export const userRelations = relations(user, ({ many }) => ({
  accountLinks: many(accountLinks),
  accounts: many(account),
  chesscomRequestBudgets: many(chesscomRequestBudgets),
  coachConversations: many(coachConversations),
  drillAttempts: many(drillAttempts),
  drillItems: many(drillItems),
  focuses: many(focuses),
  sessions: many(session),
}));

export const weaknessChangeNotesRelations = relations(weaknessChangeNotes, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [weaknessChangeNotes.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const weaknessEstimatesRelations = relations(weaknessEstimates, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [weaknessEstimates.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const weaknessSeriesPointRelations = relations(weaknessSeriesPoint, ({ one }) => ({
  game: one(games, {
    fields: [weaknessSeriesPoint.gameId],
    references: [games.id],
  }),
  linkedAccount: one(linkedAccounts, {
    fields: [weaknessSeriesPoint.accountId],
    references: [linkedAccounts.id],
  }),
}));

export const weaknessStateHistoryRelations = relations(weaknessStateHistory, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [weaknessStateHistory.accountId],
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
