import { relations } from "drizzle-orm/relations";
import {
  bandReference,
  gameMetrics,
  gameOpenings,
  gamePreviews,
  games,
  importRuns,
  kpiSnapshot,
  linkedAccounts,
  moveFeatures,
  moves,
  openingExplorer,
  openingLines,
  referenceBuilds,
  referenceGames,
  referenceObservations,
  repertoireNodes,
  weaknessEstimates,
  weaknessSeriesPoint,
  weaknessStateHistory,
} from "./schema.js";

export const bandReferenceRelations = relations(bandReference, ({ one }) => ({
  referenceBuild: one(referenceBuilds, {
    fields: [bandReference.buildId],
    references: [referenceBuilds.id],
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
  gameMetrics: many(gameMetrics),
  gameOpenings: many(gameOpenings),
  gamePreviews: many(gamePreviews),
  linkedAccount: one(linkedAccounts, {
    fields: [games.accountId],
    references: [linkedAccounts.id],
  }),
  moveFeatures: many(moveFeatures),
  moves: many(moves),
  weaknessSeriesPoints: many(weaknessSeriesPoint),
}));

export const importRunsRelations = relations(importRuns, ({ one }) => ({
  linkedAccount: one(linkedAccounts, {
    fields: [importRuns.accountId],
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
  gameMetrics: many(gameMetrics),
  gameOpenings: many(gameOpenings),
  games: many(games),
  importRuns: many(importRuns),
  kpiSnapshots: many(kpiSnapshot),
  openingLines: many(openingLines),
  repertoireNodes: many(repertoireNodes),
  weaknessEstimates: many(weaknessEstimates),
  weaknessSeriesPoints: many(weaknessSeriesPoint),
  weaknessStateHistories: many(weaknessStateHistory),
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
