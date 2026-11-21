import { describe, expect, it } from "vitest";
import type { AiUsage } from "../src/client/types.js";
import { AI_MONTHLY_BUDGET_MICRO_USD, checkBudget, costMicroUsd, MODELS } from "../src/cost.js";

const same: AiUsage = {
  inputTokens: 1000,
  outputTokens: 500,
  cacheCreationTokens: 2000,
  cacheReadTokens: 10000,
};

describe("costMicroUsd", () => {
  it("prices sonnet with cache tiers", () => {
    expect(costMicroUsd("claude-sonnet-5-5", same)).toBe(14000);
  });
  it("halves for batch", () => {
    expect(costMicroUsd("claude-sonnet-5-5", same, { batch: true })).toBe(7000);
  });
  it("prices haiku", () => {
    const u = { inputTokens: 1000, outputTokens: 1000, cacheCreationTokens: 0, cacheReadTokens: 0 };
    expect(costMicroUsd("claude-haiku-5-5", u)).toBe(600);
  });
  it("applies the haiku long-prompt tier above 100k prompt tokens", () => {
    const u = {
      inputTokens: 150_000,
      outputTokens: 1000,
      cacheCreationTokens: 0,
      cacheReadTokens: 0,
    };
    expect(costMicroUsd("claude-haiku-5-5", u)).toBe(77500);
  });
  it("counts cache tokens toward the long-prompt threshold", () => {
    const u = {
      inputTokens: 1000,
      outputTokens: 0,
      cacheCreationTokens: 0,
      cacheReadTokens: 200_000,
    };
    expect(costMicroUsd("claude-haiku-5-5", u)).toBe(500 + 200_000 * 0.05);
  });
  it("rounds to an integer", () => {
    const u = { inputTokens: 1, outputTokens: 0, cacheCreationTokens: 1, cacheReadTokens: 0 };
    expect(Number.isInteger(costMicroUsd("claude-haiku-5-5", u, { batch: true }))).toBe(true);
  });
  it("exposes the model map", () => {
    expect(MODELS).toEqual({
      report: "claude-sonnet-5-5",
      chat: "claude-sonnet-5-5",
      explain: "claude-haiku-5-5",
      weekly: "claude-haiku-5-5",
    });
  });
});

describe("checkBudget", () => {
  it("hard-stops at the plan budget", () => {
    expect(checkBudget(2_000_000, "pro")).toEqual({ ok: false, reason: "budget" });
    expect(checkBudget(1_999_999, "pro")).toEqual({ ok: true });
    expect(checkBudget(200_000, "free")).toEqual({ ok: false, reason: "budget" });
    expect(AI_MONTHLY_BUDGET_MICRO_USD).toEqual({ free: 200_000, pro: 2_000_000 });
  });
});
