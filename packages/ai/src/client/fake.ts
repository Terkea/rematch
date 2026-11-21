import type Anthropic from "@anthropic-ai/sdk";
import type {
  AiClient,
  AiUsage,
  BatchResultItem,
  ChatTurnRequest,
  StructuredRequest,
  StructuredResult,
} from "./types.js";

export interface FakeStructured {
  parsed: unknown | null;
  stopReason?: string;
  usage?: Partial<AiUsage>;
}

export interface FakeTurn {
  deltas: string[];
  toolUses?: { id: string; name: string; input: unknown }[];
  stopReason?: string;
  usage?: Partial<AiUsage>;
}

export interface FakeBatch {
  status: "in_progress" | "canceling" | "ended";
  results: BatchResultItem[];
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
      const out = script.structured(req, structuredCalls++);
      return {
        parsed: out.parsed,
        content: [{ type: "text", text: JSON.stringify(out.parsed), citations: null }],
        stopReason: out.stopReason ?? "end_turn",
        usage: zeroUsage(out.usage),
        model: req.model,
      } as StructuredResult;
    },
    async streamTurn(req, onTextDelta) {
      calls.push({ kind: "turn", req });
      const fn = script.turns?.[turnCalls];
      if (!fn) throw new Error(`fake: no script for turn ${turnCalls}`);
      const turn = fn(req, turnCalls++);
      for (const d of turn.deltas) onTextDelta(d);
      const content: Anthropic.ContentBlock[] = [];
      const text = turn.deltas.join("");
      if (text) content.push({ type: "text", text, citations: null } as Anthropic.ContentBlock);
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
        const id = `batch_fake_${++batchCounter}`;
        return id;
      },
      async status(id) {
        calls.push({ kind: "batch", req: { status: id } });
        const b = script.batches?.[id];
        if (!b) throw new Error(`fake: unknown batch ${id}`);
        return b.status;
      },
      results(id) {
        calls.push({ kind: "batch", req: { results: id } });
        const b = script.batches?.[id];
        if (!b) throw new Error(`fake: unknown batch ${id}`);
        const results = b.results;
        return (async function* () {
          for (const r of results) yield r;
        })();
      },
    },
  };
}
