import type Anthropic from "@anthropic-ai/sdk";
import type { z } from "zod";
import type { AiModel } from "../cost.js";

export interface AiUsage {
  inputTokens: number;
  outputTokens: number;
  cacheCreationTokens: number;
  cacheReadTokens: number;
}

export interface StructuredRequest {
  model: AiModel;
  system: Anthropic.TextBlockParam[];
  messages: Anthropic.MessageParam[];
  maxTokens: number;
  effort: "low" | "medium" | "high";
  schema: z.ZodType;
}

export interface StructuredResult {
  parsed: unknown | null;
  content: Anthropic.ContentBlock[];
  stopReason: string | null;
  usage: AiUsage;
  model: string;
}

export interface ChatTurnRequest {
  model: AiModel;
  system: Anthropic.TextBlockParam[];
  tools: Anthropic.Tool[];
  toolChoice?: "auto" | "none";
  messages: Anthropic.MessageParam[];
  maxTokens: number;
  effort: "low" | "medium";
}

export type BatchOutcome = "succeeded" | "errored" | "canceled" | "expired";

export interface BatchResultItem {
  customId: string;
  outcome: BatchOutcome;
  result: StructuredResult | null;
  errorType: string | null;
}

export interface AiClient {
  structured(req: StructuredRequest): Promise<StructuredResult>;
  streamTurn(
    req: ChatTurnRequest,
    onTextDelta: (t: string) => void,
  ): Promise<{ message: Anthropic.Message; usage: AiUsage }>;
  batch: {
    submit(items: { customId: string; req: StructuredRequest }[]): Promise<string>;
    status(id: string): Promise<"in_progress" | "canceling" | "ended">;
    results(id: string): AsyncIterable<BatchResultItem>;
  };
}
