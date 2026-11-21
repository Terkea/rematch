import {
  createSdkMcpServer,
  type McpSdkServerConfigWithInstance,
  type SdkMcpToolDefinition,
  tool,
} from "@anthropic-ai/claude-agent-sdk";
import type { ToolCallback, ToolSpec } from "./options.js";
import { jsonSchemaToZodShape } from "./schema-to-zod.js";

/** Timeout ordering: T23's engine wait (45 s) < this fetch (55 s) < the MCP tool-call limit (60 s). */
export const CALLBACK_TIMEOUT_MS = 55_000;
export const MCP_TOOL_TIMEOUT_MS = 60_000;

type ToolResult = { content: Array<{ type: "text"; text: string }>; isError: boolean };

/** What the model sees for any callback failure: never the status, the URL or the body. */
const UNAVAILABLE: ToolResult = {
  content: [
    { type: "text", text: JSON.stringify({ available: false, reason: "tool unavailable" }) },
  ],
  isError: true,
};

async function callBack(
  cb: ToolCallback,
  name: string,
  input: unknown,
  fetchImpl: typeof fetch,
  timeoutMs: number,
): Promise<ToolResult> {
  // A plain timer (not AbortSignal.timeout) so fake timers can drive it in tests.
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), timeoutMs);
  try {
    const res = await fetchImpl(cb.url, {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${cb.token}` },
      body: JSON.stringify({ name, input }),
      signal: ac.signal,
    });
    if (!res.ok) return UNAVAILABLE;
    const reply = (await res.json()) as { content?: unknown; isError?: unknown };
    if (typeof reply.content !== "string" || typeof reply.isError !== "boolean") return UNAVAILABLE;
    return { content: [{ type: "text", text: reply.content }], isError: reply.isError };
  } catch {
    return UNAVAILABLE;
  } finally {
    clearTimeout(timer);
  }
}

/** One SDK tool per Rust tool spec. Each call posts `{name, input}` to Rust with the turn token. */
export function buildCoachTools(
  tools: ToolSpec[],
  callback: ToolCallback,
  fetchImpl: typeof fetch = fetch,
  timeoutMs = CALLBACK_TIMEOUT_MS,
): Array<SdkMcpToolDefinition> {
  return tools.map(
    (t) =>
      tool(t.name, t.description, jsonSchemaToZodShape(t.inputSchema), (args) =>
        callBack(callback, t.name, args, fetchImpl, timeoutMs),
      ) as unknown as SdkMcpToolDefinition,
  );
}

export function buildCoachServer(
  tools: ToolSpec[],
  callback: ToolCallback,
  fetchImpl: typeof fetch = fetch,
): McpSdkServerConfigWithInstance {
  return createSdkMcpServer({
    name: "coach",
    version: "1",
    tools: buildCoachTools(tools, callback, fetchImpl),
    timeout: MCP_TOOL_TIMEOUT_MS,
  });
}
