import type { AiUsage } from "./client/types.js";

export const MODELS = {
  report: "claude-sonnet-5-5",
  chat: "claude-sonnet-5-5",
  explain: "claude-haiku-5-5",
  weekly: "claude-haiku-5-5",
} as const;

export type AiModel = (typeof MODELS)[keyof typeof MODELS];

interface Rates {
  input: number;
  output: number;
  cacheWrite: number;
  cacheRead: number;
}

interface ModelPricing extends Rates {
  /** Applies when the whole prompt (input + cache write + cache read) exceeds `above` tokens. */
  longPrompt?: { above: number } & Rates;
}

/**
 * USD per MTok (= micro-USD per token). Source: Claude API pricing, the `claude-api` skill's
 * model table plus https://platform.claude.com/docs/en/about-claude/pricing, cached 2026-10-06
 * and re-checked against the skill's table on 2026-11-21 (ADR-AI-6). Cache write is the 5-minute
 * TTL rate. The Batch API is 50% of these.
 */
export const MODEL_PRICING: Record<AiModel, ModelPricing> = {
  "claude-sonnet-5-5": { input: 2.0, output: 10.0, cacheWrite: 2.5, cacheRead: 0.2 },
  "claude-haiku-5-5": {
    input: 0.1,
    output: 0.5,
    cacheWrite: 0.125,
    cacheRead: 0.01,
    longPrompt: { above: 100_000, input: 0.5, output: 2.5, cacheWrite: 0.625, cacheRead: 0.05 },
  },
};

export function costMicroUsd(model: AiModel, u: AiUsage, opts?: { batch?: boolean }): number {
  const p = MODEL_PRICING[model];
  const prompt = u.inputTokens + u.cacheCreationTokens + u.cacheReadTokens;
  const r: Rates = p.longPrompt && prompt > p.longPrompt.above ? p.longPrompt : p;
  const raw =
    u.inputTokens * r.input +
    u.outputTokens * r.output +
    u.cacheCreationTokens * r.cacheWrite +
    u.cacheReadTokens * r.cacheRead;
  return Math.round(opts?.batch ? raw / 2 : raw);
}

export const AI_MONTHLY_BUDGET_MICRO_USD = { free: 200_000, pro: 2_000_000 };

export function checkBudget(
  spentMicroUsd: number,
  plan: "free" | "pro",
  budgets: typeof AI_MONTHLY_BUDGET_MICRO_USD = AI_MONTHLY_BUDGET_MICRO_USD,
): { ok: true } | { ok: false; reason: "budget" } {
  return spentMicroUsd >= budgets[plan] ? { ok: false, reason: "budget" } : { ok: true };
}
