import type { Options, SDKMessage } from "@anthropic-ai/claude-agent-sdk";
import { loadConfig, type SidecarConfig } from "../src/config.js";
import type { LogFields, Logger } from "../src/log.js";

export const TOKEN = "test-sidecar-token-0123456789abcdef";

export function testConfig(extra: NodeJS.ProcessEnv = {}): SidecarConfig {
  return loadConfig({
    PATH: "/usr/bin",
    AI_SIDECAR_TOKEN: TOKEN,
    ANTHROPIC_API_KEY: "sk-ant-test-SECRET123",
    AI_SIDECAR_WORK_DIR: "/tmp/ai-sidecar-test/work",
    AI_SIDECAR_CONFIG_DIR: "/tmp/ai-sidecar-test/claude",
    ...extra,
  });
}

export type Captured = { prompt: unknown; options: Options };

/** A fake `query()` that yields scripted messages and records what it was called with. */
export function scripted(
  scripts: Array<SDKMessage[] | ((signal: AbortSignal) => AsyncGenerator<SDKMessage>)>,
): {
  fn: (p: { prompt: unknown; options?: Options }) => AsyncGenerator<SDKMessage, void>;
  calls: Captured[];
} {
  const calls: Captured[] = [];
  let i = 0;
  const fn = (p: { prompt: unknown; options?: Options }) => {
    const options = p.options ?? {};
    calls.push({ prompt: p.prompt, options });
    const script = scripts[Math.min(i, scripts.length - 1)];
    i += 1;
    if (typeof script === "function") {
      const signal = options.abortController?.signal ?? new AbortController().signal;
      return script(signal);
    }
    return (async function* () {
      for (const m of script ?? []) yield m;
    })();
  };
  return { fn, calls };
}

export function usage(input = 12, output = 800, cacheRead = 5100, cacheCreate = 0) {
  return {
    inputTokens: input,
    outputTokens: output,
    cacheReadInputTokens: cacheRead,
    cacheCreationInputTokens: cacheCreate,
    webSearchRequests: 0,
    costUSD: 0.01,
    contextWindow: 200000,
    maxOutputTokens: 64000,
  };
}

export function result(over: Record<string, unknown> = {}): SDKMessage {
  return {
    type: "result",
    subtype: "success",
    duration_ms: 4210,
    duration_api_ms: 4000,
    is_error: false,
    num_turns: 1,
    result: "ok text",
    stop_reason: "end_turn",
    total_cost_usd: 0.0123,
    usage: {},
    modelUsage: { "claude-sonnet-5-5": usage() },
    permission_denials: [],
    uuid: "00000000-0000-4000-8000-000000000001",
    session_id: "00000000-0000-4000-8000-000000000002",
    ...over,
  } as unknown as SDKMessage;
}

export function textDelta(text: string, parent: string | null = null): SDKMessage {
  return {
    type: "stream_event",
    event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text } },
    parent_tool_use_id: parent,
    uuid: "00000000-0000-4000-8000-000000000003",
    session_id: "s",
  } as unknown as SDKMessage;
}

export function assistantToolUse(id: string, name: string, input: unknown): SDKMessage {
  return {
    type: "assistant",
    message: { content: [{ type: "tool_use", id, name, input }] },
    parent_tool_use_id: null,
    uuid: "00000000-0000-4000-8000-000000000004",
    session_id: "s",
  } as unknown as SDKMessage;
}

export function captureLogger(): Logger & { lines: string[] } {
  const lines: string[] = [];
  const push = (f: LogFields) => {
    lines.push(JSON.stringify(f));
  };
  return { lines, info: push, warn: push, error: push };
}

/** Minimal SSE sink standing in for a ServerResponse. */
export class FakeRes {
  status = 0;
  headers: Record<string, string> = {};
  chunks: string[] = [];
  headersSent = false;
  writableEnded = false;
  private closeHandlers: Array<() => void> = [];
  writeHead(status: number, headers: Record<string, string>) {
    this.status = status;
    this.headers = headers;
    this.headersSent = true;
    return this;
  }
  write(chunk: string) {
    this.chunks.push(chunk);
    return true;
  }
  end(chunk?: string) {
    if (chunk) this.chunks.push(chunk);
    this.writableEnded = true;
    return this;
  }
  on(event: string, fn: () => void) {
    if (event === "close") this.closeHandlers.push(fn);
    return this;
  }
  disconnect() {
    for (const fn of this.closeHandlers) fn();
  }
  body() {
    return this.chunks.join("");
  }
  events(): Array<{ event: string; data: unknown }> {
    return this.body()
      .split("\n\n")
      .filter((b) => b.startsWith("event:"))
      .map((b) => {
        const [ev, data] = b.split("\n");
        return {
          event: (ev ?? "").slice("event: ".length),
          data: JSON.parse((data ?? "").slice("data: ".length)),
        };
      });
  }
}

export const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
