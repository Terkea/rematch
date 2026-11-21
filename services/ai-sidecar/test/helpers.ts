import { randomUUID } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import type { Options, SDKMessage, SessionStore } from "@anthropic-ai/claude-agent-sdk";
import { loadConfig, type SidecarConfig } from "../src/config.js";
import type { LogFields, Logger } from "../src/log.js";

export const TOKEN = "test-sidecar-token-0123456789abcdef";

export function testConfig(extra: NodeJS.ProcessEnv = {}): SidecarConfig {
  return loadConfig({
    PATH: "/usr/bin",
    AI_SIDECAR_ENV: "test",
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
  flushed = 0;
  private closeHandlers: Array<() => void> = [];
  writeHead(status: number, headers: Record<string, string>) {
    this.status = status;
    this.headers = headers;
    this.headersSent = true;
    return this;
  }
  flushHeaders() {
    this.flushed += 1;
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

export const tempDir = (prefix = "ai-sidecar-") => mkdtempSync(join(tmpdir(), prefix));

type Usage4 = { input: number; output: number; cacheRead: number; cacheCreate: number };

/**
 * A fake `query()` that behaves like the CLI where sessions are concerned:
 * - `sessionId` fails with "Session ID … is already in use" when a local transcript exists;
 * - `resume` calls `sessionStore.load`, and fails (error_during_execution) when neither the store
 *   nor the local disk has the transcript, or when `corruptResume` is set;
 * - every turn writes the local transcript under CLAUDE_CONFIG_DIR and appends entries to the store;
 * - it streams `message_start` / `message_delta` usage, and the result's `modelUsage` is the
 *   running session total, cumulative across resumed turns, as the real SDK reports it.
 */
export class FakeCli {
  calls: Captured[] = [];
  corruptResume = false;
  /** Temporary resume dirs used (and already deleted), for assertions. */
  scratchDirs: string[] = [];
  private totals = new Map<string, Usage4>();

  constructor(
    readonly turn: Usage4 = { input: 10, output: 20, cacheRead: 100, cacheCreate: 5 },
    readonly model = "claude-sonnet-5-5",
  ) {}

  localPath(options: Options, id: string): string {
    const env = options.env ?? {};
    return join(
      env.CLAUDE_CONFIG_DIR ?? "",
      "projects",
      env.CLAUDE_CODE_PROJECT_DIR_NAME ?? "",
      `${id}.jsonl`,
    );
  }

  fn = (p: { prompt: unknown; options?: Options }): AsyncGenerator<SDKMessage, void> => {
    const options = p.options ?? {};
    this.calls.push({ prompt: p.prompt, options });
    const self = this;
    return (async function* () {
      const store = options.sessionStore as SessionStore | undefined;
      const projectKey = options.env?.CLAUDE_CODE_PROJECT_DIR_NAME ?? "";
      const id = options.resume ?? options.sessionId ?? randomUUID();
      const local = self.localPath(options, id);
      let prev: Usage4 = { input: 0, output: 0, cacheRead: 0, cacheCreate: 0 };
      // Where this run writes its transcript. A resume served from the store runs in a temporary
      // config dir that the SDK deletes at run end, as the real SDK does.
      let writeTo = local;
      let scratch: string | undefined;
      if (options.resume) {
        const loaded = store ? await store.load({ projectKey, sessionId: id }) : null;
        if (loaded && !self.corruptResume) {
          scratch = tempDir("fake-sdk-resume-");
          writeTo = join(scratch, "projects", projectKey, `${id}.jsonl`);
        }
        if (self.corruptResume || (!loaded && !existsSync(local))) {
          yield result({
            subtype: "error_during_execution",
            is_error: true,
            errors: [`No conversation found with session ID: ${id}`],
            modelUsage: {},
          });
          return;
        }
        prev = self.totals.get(id) ?? prev;
      } else if (existsSync(local)) {
        throw new Error(`Session ID ${id} is already in use`);
      }
      const t = self.turn;
      const msgId = `msg_${randomUUID()}`;
      yield {
        type: "stream_event",
        event: {
          type: "message_start",
          message: {
            id: msgId,
            model: self.model,
            usage: {
              input_tokens: t.input,
              output_tokens: 1,
              cache_read_input_tokens: t.cacheRead,
              cache_creation_input_tokens: t.cacheCreate,
            },
          },
        },
        parent_tool_use_id: null,
        uuid: randomUUID(),
        session_id: id,
      } as unknown as SDKMessage;
      yield textDelta("reply");
      yield {
        type: "stream_event",
        event: {
          type: "message_delta",
          delta: { stop_reason: "end_turn" },
          usage: { output_tokens: t.output },
        },
        parent_tool_use_id: null,
        uuid: randomUUID(),
        session_id: id,
      } as unknown as SDKMessage;
      mkdirSync(dirname(writeTo), { recursive: true });
      writeFileSync(writeTo, '{"type":"user"}\n');
      if (scratch) {
        rmSync(scratch, { recursive: true, force: true });
        self.scratchDirs.push(scratch);
      }
      await store?.append({ projectKey, sessionId: id }, [
        { type: "user", uuid: randomUUID() },
        { type: "assistant", uuid: randomUUID() },
      ]);
      const total = {
        input: prev.input + t.input,
        output: prev.output + t.output,
        cacheRead: prev.cacheRead + t.cacheRead,
        cacheCreate: prev.cacheCreate + t.cacheCreate,
      };
      self.totals.set(id, total);
      yield result({
        session_id: id,
        modelUsage: {
          [self.model]: usage(total.input, total.output, total.cacheRead, total.cacheCreate),
        },
      });
    })();
  };
}
