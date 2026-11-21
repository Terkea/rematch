import Anthropic from "@anthropic-ai/sdk";
import { zodOutputFormat } from "@anthropic-ai/sdk/helpers/zod";
import type { z } from "zod";
import {
  AiUnavailableError,
  classifyAiError,
  classifyBatchErrorType,
  classifyStreamError,
} from "./errors.js";
import { parseModelOutput } from "./parse.js";
import type {
  AiClient,
  AiUsage,
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

function toResult(message: Anthropic.Message, schema: z.ZodType | undefined): StructuredResult {
  const o = parseModelOutput(message.content, message.stop_reason, schema);
  return {
    parsed: o.parsed,
    parseError: o.parseError,
    parseIssues: o.parseIssues,
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
        return toResult(message, req.schema);
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

    batch: {
      submit: (items) =>
        wrap(async () => {
          const batch = await client.messages.batches.create({
            requests: items.map((i) => ({
              custom_id: i.customId,
              params: structuredParams(i.req),
            })),
          });
          return batch.id;
        }),
      status: (id) =>
        wrap(async () => (await client.messages.batches.retrieve(id)).processing_status),
      results: (id, resolveSchema): AsyncIterable<BatchResultItem> => ({
        async *[Symbol.asyncIterator]() {
          const stream = await wrap(() => client.messages.batches.results(id));
          const iterator = stream[Symbol.asyncIterator]();
          for (;;) {
            let next: IteratorResult<Anthropic.Messages.MessageBatchIndividualResponse>;
            try {
              next = await iterator.next();
            } catch (e) {
              throw classifyStreamError(e);
            }
            if (next.done) return;
            const line = next.value;
            const r = line.result;
            if (r.type !== "succeeded") {
              const errorType = r.type === "errored" ? (r.error?.error?.type ?? "unknown") : null;
              yield {
                customId: line.custom_id,
                outcome: r.type,
                result: null,
                errorType,
                errorKind: r.type === "errored" ? classifyBatchErrorType(errorType) : null,
                parseError: null,
              };
              continue;
            }
            const result = toResult(r.message, resolveSchema(line.custom_id));
            yield {
              customId: line.custom_id,
              outcome: "succeeded",
              result,
              errorType: null,
              errorKind: null,
              parseError: result.parseError,
            };
          }
        },
      }),
    },
  };
}
