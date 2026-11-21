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

export type ParseError =
  | "no_text"
  | "invalid_json"
  | "schema_mismatch"
  | "refusal"
  | "truncated"
  | "no_schema";

/** Zod issue location and code only; never model content. */
export interface ParseIssue {
  path: string;
  code: string;
}

export type AiErrorKind = "retryable" | "fatal" | "unknown";

export type SchemaResolver = (customId: string) => z.ZodType | undefined;

export interface StructuredResult {
  parsed: unknown | null;
  /** Why `parsed` is null; null when parsing succeeded. */
  parseError: ParseError | null;
  parseIssues: ParseIssue[] | null;
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
  /** Taxonomy of `errorType` for errored items; null otherwise. */
  errorKind: AiErrorKind | null;
  /** Mirrors `result.parseError` for succeeded items. */
  parseError: ParseError | null;
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
    results(id: string, resolveSchema: SchemaResolver): AsyncIterable<BatchResultItem>;
  };
}
