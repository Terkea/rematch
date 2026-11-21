import type Anthropic from "@anthropic-ai/sdk";
import { classifyBatchErrorType } from "./errors.js";
import { parseModelOutput } from "./parse.js";
import type {
  AiClient,
  AiUsage,
  BatchOutcome,
  BatchResultItem,
  ChatTurnRequest,
  SchemaResolver,
  StructuredRequest,
  StructuredResult,
} from "./types.js";

/**
 * Scripted structured output. The text goes through the same parse path as the real adapter
 * (`parseModelOutput` with the request's schema), so schema mismatches, invalid JSON, refusals
 * and truncation are all scriptable. `parsed` is a shorthand for `text: JSON.stringify(parsed)`;
 * `parsed: null` with no `text` means no text block. Throw to script 429/overloaded/etc.
 */
export interface FakeStructured {
  parsed?: unknown | null;
  text?: string;
  stopReason?: string;
  usage?: Partial<AiUsage>;
}

export interface FakeTurn {
  deltas: string[];
  toolUses?: { id: string; name: string; input: unknown }[];
  stopReason?: string;
  usage?: Partial<AiUsage>;
}

export interface FakeBatchItem {
  customId: string;
  outcome: BatchOutcome;
  /** Raw model text for succeeded items, parsed with the schema from the resolver. */
  text?: string;
  stopReason?: string;
  usage?: Partial<AiUsage>;
  /** For errored items. */
  errorType?: string;
}

export interface FakeBatch {
  status: "in_progress" | "canceling" | "ended";
  results: FakeBatchItem[];
}

export interface FakeScript {
  structured?: (req: StructuredRequest, call: number) => FakeStructured;
  turns?: ((req: ChatTurnRequest, call: number) => FakeTurn)[];
  batches?: Record<string, FakeBatch>;
}

const zeroUsage = (p?: Partial<AiUsage>): AiUsage => ({
  inputTokens: p?.inputTokens ?? 0,
  outputTokens: p?.outputTokens ?? 0,
  cacheCreationTokens: p?.cacheCreationTokens ?? 0,
  cacheReadTokens: p?.cacheReadTokens ?? 0,
});

function textBlocks(text: string | undefined): Anthropic.ContentBlock[] {
  return text === undefined
    ? []
    : [{ type: "text", text, citations: null } as Anthropic.ContentBlock];
}

function structuredResult(
  schema: StructuredRequest["schema"] | undefined,
  model: string,
  out: FakeStructured,
): StructuredResult {
  const text = out.text ?? (out.parsed == null ? undefined : JSON.stringify(out.parsed));
  const content = textBlocks(text);
  const stopReason = out.stopReason ?? "end_turn";
  const o = parseModelOutput(content, stopReason, schema);
  return {
    parsed: o.parsed,
    parseError: o.parseError,
    parseIssues: o.parseIssues,
    content,
    stopReason,
    usage: zeroUsage(out.usage),
    model,
  };
}

export function createFakeAiClient(
  script: FakeScript,
): AiClient & { calls: { kind: "structured" | "turn" | "batch"; req: unknown }[] } {
  const calls: { kind: "structured" | "turn" | "batch"; req: unknown }[] = [];
  let structuredCalls = 0;
  let turnCalls = 0;
  let batchCounter = 0;

  return {
    calls,
    async structured(req): Promise<StructuredResult> {
      calls.push({ kind: "structured", req });
      if (!script.structured) throw new Error("fake: no structured script");
      return structuredResult(req.schema, req.model, script.structured(req, structuredCalls++));
    },
    async streamTurn(req, onTextDelta) {
      calls.push({ kind: "turn", req });
      const fn = script.turns?.[turnCalls];
      if (!fn) throw new Error(`fake: no script for turn ${turnCalls}`);
      const turn = fn(req, turnCalls++);
      for (const d of turn.deltas) onTextDelta(d);
      const content: Anthropic.ContentBlock[] = [];
      const text = turn.deltas.join("");
      if (text) content.push(...textBlocks(text));
      for (const t of turn.toolUses ?? []) {
        content.push({
          type: "tool_use",
          id: t.id,
          name: t.name,
          input: t.input,
        } as Anthropic.ContentBlock);
      }
      const usage = zeroUsage(turn.usage);
      const message = {
        id: `msg_fake_${turnCalls}`,
        type: "message",
        role: "assistant",
        model: req.model,
        content,
        stop_reason: turn.stopReason ?? (turn.toolUses?.length ? "tool_use" : "end_turn"),
        stop_sequence: null,
        usage: {
          input_tokens: usage.inputTokens,
          output_tokens: usage.outputTokens,
          cache_creation_input_tokens: usage.cacheCreationTokens,
          cache_read_input_tokens: usage.cacheReadTokens,
        },
      } as unknown as Anthropic.Message;
      return { message, usage };
    },
    batch: {
      async submit(items) {
        calls.push({ kind: "batch", req: items });
        return `batch_fake_${++batchCounter}`;
      },
      async status(id) {
        calls.push({ kind: "batch", req: { status: id } });
        const b = script.batches?.[id];
        if (!b) throw new Error(`fake: unknown batch ${id}`);
        return b.status;
      },
      results(id, resolveSchema: SchemaResolver) {
        calls.push({ kind: "batch", req: { results: id } });
        const b = script.batches?.[id];
        if (!b) throw new Error(`fake: unknown batch ${id}`);
        const items = b.results;
        return (async function* (): AsyncGenerator<BatchResultItem> {
          for (const i of items) {
            if (i.outcome !== "succeeded") {
              yield {
                customId: i.customId,
                outcome: i.outcome,
                result: null,
                errorType: i.errorType ?? null,
                errorKind: i.outcome === "errored" ? classifyBatchErrorType(i.errorType) : null,
                parseError: null,
              };
              continue;
            }
            const result = structuredResult(resolveSchema(i.customId), "fake", {
              ...(i.text !== undefined ? { text: i.text } : {}),
              ...(i.stopReason !== undefined ? { stopReason: i.stopReason } : {}),
              ...(i.usage !== undefined ? { usage: i.usage } : {}),
            });
            yield {
              customId: i.customId,
              outcome: "succeeded",
              result,
              errorType: null,
              errorKind: null,
              parseError: result.parseError,
            };
          }
        })();
      },
    },
  };
}
