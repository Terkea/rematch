import Anthropic from "@anthropic-ai/sdk";
import { zodOutputFormat } from "@anthropic-ai/sdk/helpers/zod";
import type { z } from "zod";
import { AiUnavailableError, classifyAiError } from "./errors.js";
import type {
  AiClient,
  AiUsage,
  BatchOutcome,
  BatchResultItem,
  ChatTurnRequest,
  StructuredRequest,
  StructuredResult,
} from "./types.js";

export interface AnthropicAiClientOptions {
  apiKey: string;
  fetch?: typeof fetch;
  maxRetries?: number;
  timeoutMs?: number;
}

function toUsage(u: Anthropic.Usage): AiUsage {
  return {
    inputTokens: u.input_tokens ?? 0,
    outputTokens: u.output_tokens ?? 0,
    cacheCreationTokens: u.cache_creation_input_tokens ?? 0,
    cacheReadTokens: u.cache_read_input_tokens ?? 0,
  };
}

function safeParse(schema: z.ZodType, text: string): unknown | null {
  try {
    const r = schema.safeParse(JSON.parse(text));
    return r.success ? r.data : null;
  } catch {
    return null;
  }
}

function firstText(content: Anthropic.ContentBlock[]): string | null {
  for (const b of content) if (b.type === "text") return b.text;
  return null;
}

function toResult(message: Anthropic.Message, parsed: unknown | null): StructuredResult {
  return {
    parsed,
    content: message.content,
    stopReason: message.stop_reason,
    usage: toUsage(message.usage),
    model: message.model,
  };
}

function structuredParams(req: StructuredRequest) {
  return {
    model: req.model,
    max_tokens: req.maxTokens,
    system: req.system,
    messages: req.messages,
    output_config: { effort: req.effort, format: zodOutputFormat(req.schema) },
  };
}

export function createAnthropicAiClient(opts: AnthropicAiClientOptions): AiClient {
  if (!opts.apiKey) throw new AiUnavailableError("no_api_key");
  const client = new Anthropic({
    apiKey: opts.apiKey,
    maxRetries: opts.maxRetries ?? 2,
    timeout: opts.timeoutMs ?? 120_000,
    ...(opts.fetch ? { fetch: opts.fetch } : {}),
  });

  async function wrap<T>(fn: () => Promise<T>): Promise<T> {
    try {
      return await fn();
    } catch (e) {
      throw classifyAiError(e);
    }
  }

  return {
    structured: (req) =>
      wrap(async () => {
        // `messages.parse` throws on refusals and truncated JSON, losing the message (and its usage).
        // So send the same zodOutputFormat via `create` and parse ourselves; null = failed attempt.
        const message = await client.messages.create(structuredParams(req));
        const text = firstText(message.content);
        const usable = message.stop_reason !== "refusal" && message.stop_reason !== "max_tokens";
        return toResult(message, usable && text !== null ? safeParse(req.schema, text) : null);
      }),

    streamTurn: (req: ChatTurnRequest, onTextDelta) =>
      wrap(async () => {
        const stream = client.messages.stream({
          model: req.model,
          max_tokens: req.maxTokens,
          system: req.system,
          tools: req.tools,
          tool_choice: { type: req.toolChoice ?? "auto" },
          messages: req.messages,
          output_config: { effort: req.effort },
        });
        stream.on("text", (delta) => onTextDelta(delta));
        const message = await stream.finalMessage();
        return { message, usage: toUsage(message.usage) };
      }),

    batch: (() => {
      const registry = new Map<string, Map<string, z.ZodType>>();
      return {
        submit: (items: { customId: string; req: StructuredRequest }[]) =>
          wrap(async () => {
            const batch = await client.messages.batches.create({
              requests: items.map((i) => ({
                custom_id: i.customId,
                params: structuredParams(i.req),
              })),
            });
            registry.set(batch.id, new Map(items.map((i) => [i.customId, i.req.schema])));
            return batch.id;
          }),
        status: (id: string) =>
          wrap(async () => (await client.messages.batches.retrieve(id)).processing_status),
        results: (id: string): AsyncIterable<BatchResultItem> => ({
          async *[Symbol.asyncIterator]() {
            const schemas = registry.get(id);
            const stream = await wrap(() => client.messages.batches.results(id));
            for await (const line of stream) {
              const r = line.result;
              const outcome: BatchOutcome = r.type;
              if (r.type !== "succeeded") {
                const errorType = r.type === "errored" ? (r.error?.error?.type ?? "unknown") : null;
                yield { customId: line.custom_id, outcome, result: null, errorType };
                continue;
              }
              const schema = schemas?.get(line.custom_id);
              const text = firstText(r.message.content);
              const parsed =
                schema && text !== null && r.message.stop_reason !== "refusal"
                  ? safeParse(schema, text)
                  : null;
              yield {
                customId: line.custom_id,
                outcome,
                result: toResult(r.message, parsed),
                errorType: null,
              };
            }
          },
        }),
      };
    })(),
  };
}
