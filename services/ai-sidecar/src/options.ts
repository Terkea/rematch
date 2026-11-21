import type {
  EffortLevel,
  McpSdkServerConfigWithInstance,
  Options,
  SessionStore,
} from "@anthropic-ai/claude-agent-sdk";
import type { SidecarConfig } from "./config.js";
import { SidecarError } from "./errors.js";

/** Full model ids only: an alias would let the CLI pick a model we did not price (ADR-R3-6). */
export const MODEL_ALLOWLIST = ["claude-sonnet-5-5", "claude-haiku-5-5"] as const;
export type ModelId = (typeof MODEL_ALLOWLIST)[number];
const EFFORTS: readonly EffortLevel[] = ["low", "medium", "high", "xhigh", "max"];
export const MAX_TURNS_CAP = 10;
export const CLIENT_APP = "chess-improver-sidecar/0.1";
/**
 * Fixed project directory name: the session store's `projectKey` is derived from it rather than
 * from the cwd, so every replica (and every workDir path) reads and writes the same keys.
 */
export const PROJECT_DIR_NAME = "chess-improver-coach";

export type TextBlock = { type: "text"; text: string };
export type OutputFormat = { type: "json_schema"; schema: Record<string, unknown> };

export type BaseRequest = {
  model: ModelId;
  effort: EffortLevel;
  maxTokens: number;
  timeoutMs: number;
  maxTurns: number;
  systemPrompt: string;
  prompt: TextBlock[];
};
export type QueryRequest = BaseRequest & { outputFormat?: OutputFormat };
export type ToolSpec = { name: string; description: string; inputSchema: unknown };
export type ToolCallback = { url: string; token: string };
export type ChatRequest = BaseRequest & {
  tools: ToolSpec[];
  /** Required when `tools` is not empty; a turn without tools may omit it. */
  toolCallback?: ToolCallback;
  /** The Postgres conversation id; it is also the SDK session id. */
  conversationId: string;
  /** False on the first turn of a conversation, true on every later turn. */
  resume: boolean;
  /** Summary plus recent turns, used only when the session cannot be resumed. */
  rebuildContext?: string;
};

const bad = (message: string) => new SidecarError(400, "invalid_request_error", message);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const TOOL_NAME = /^[a-zA-Z0-9_-]{1,64}$/;

function obj(v: unknown, what: string): Record<string, unknown> {
  if (typeof v !== "object" || v === null || Array.isArray(v))
    throw bad(`${what} must be an object`);
  return v as Record<string, unknown>;
}
function int(v: unknown, what: string, min: number): number {
  if (typeof v !== "number" || !Number.isInteger(v) || v < min)
    throw bad(`${what} must be an integer >= ${min}`);
  return v;
}
function str(v: unknown, what: string): string {
  if (typeof v !== "string") throw bad(`${what} must be a string`);
  return v;
}

function parseBase(b: Record<string, unknown>): BaseRequest {
  const model = b.model;
  if (typeof model !== "string" || !(MODEL_ALLOWLIST as readonly string[]).includes(model)) {
    throw bad("model is not allowed");
  }
  if (typeof b.effort !== "string" || !EFFORTS.includes(b.effort as EffortLevel)) {
    throw bad("effort is invalid");
  }
  if (!Array.isArray(b.prompt) || b.prompt.length === 0)
    throw bad("prompt must be a non-empty array");
  const prompt = b.prompt.map((blk, i) => {
    const o = obj(blk, `prompt[${i}]`);
    if (o.type !== "text") throw bad(`prompt[${i}].type must be text`);
    return { type: "text" as const, text: str(o.text, `prompt[${i}].text`) };
  });
  return {
    model: model as ModelId,
    effort: b.effort as EffortLevel,
    maxTokens: int(b.maxTokens, "maxTokens", 1),
    timeoutMs: int(b.timeoutMs, "timeoutMs", 1),
    maxTurns: int(b.maxTurns, "maxTurns", 1),
    systemPrompt: str(b.systemPrompt, "systemPrompt"),
    prompt,
  };
}

export function parseQueryRequest(body: unknown): QueryRequest {
  const b = obj(body, "body");
  const req: QueryRequest = parseBase(b);
  if (b.outputFormat !== undefined) {
    const f = obj(b.outputFormat, "outputFormat");
    if (f.type !== "json_schema") throw bad("outputFormat.type must be json_schema");
    req.outputFormat = { type: "json_schema", schema: obj(f.schema, "outputFormat.schema") };
  }
  return req;
}

export function parseChatRequest(body: unknown): ChatRequest {
  const b = obj(body, "body");
  if (b.outputFormat !== undefined) throw bad("outputFormat is not allowed on /v1/chat");
  const base = parseBase(b);
  if (!Array.isArray(b.tools)) throw bad("tools must be an array");
  const tools = b.tools.map((t, i) => {
    const o = obj(t, `tools[${i}]`);
    const name = str(o.name, `tools[${i}].name`);
    if (!TOOL_NAME.test(name)) throw bad(`tools[${i}].name is invalid`);
    return {
      name,
      description: str(o.description, `tools[${i}].description`),
      inputSchema: o.inputSchema,
    };
  });
  let toolCallback: ToolCallback | undefined;
  if (b.toolCallback !== undefined || tools.length > 0) {
    const cb = obj(b.toolCallback, "toolCallback");
    const url = str(cb.url, "toolCallback.url");
    if (!/^https?:\/\//.test(url)) throw bad("toolCallback.url must be http(s)");
    toolCallback = { url, token: str(cb.token, "toolCallback.token") };
  }
  const conversationId = str(b.conversationId, "conversationId");
  if (!UUID.test(conversationId)) throw bad("conversationId must be a uuid");
  if (b.resume !== undefined && typeof b.resume !== "boolean")
    throw bad("resume must be a boolean");
  const req: ChatRequest = {
    ...base,
    tools,
    ...(toolCallback ? { toolCallback } : {}),
    conversationId: conversationId.toLowerCase(),
    resume: b.resume === true,
  };
  if (b.rebuildContext !== undefined) req.rebuildContext = str(b.rebuildContext, "rebuildContext");
  if (req.resume && !req.rebuildContext) {
    throw bad("rebuildContext is required when resume is true");
  }
  return req;
}

export type SessionChoice =
  | { sessionId: string; resume?: undefined; store?: SessionStore }
  | { resume: string; sessionId?: undefined; store?: SessionStore };

export type OptionExtras = {
  abortController: AbortController;
  mcpServer?: McpSdkServerConfigWithInstance;
  toolNames?: string[];
  partial: boolean;
  /** Chat only. Without it the call is stateless (`persistSession: false`). */
  session?: SessionChoice;
};

export function childEnv(req: BaseRequest, cfg: SidecarConfig): Record<string, string> {
  return {
    ...cfg.childBaseEnv,
    HOME: cfg.configDir,
    CLAUDE_CONFIG_DIR: cfg.configDir,
    CLAUDE_CODE_DISABLE_AUTO_MEMORY: "1",
    CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
    CLAUDE_CODE_MAX_OUTPUT_TOKENS: String(req.maxTokens),
    CLAUDE_AGENT_SDK_CLIENT_APP: CLIENT_APP,
    CLAUDE_CODE_PROJECT_DIR_NAME: PROJECT_DIR_NAME,
  };
}

/** The single place where the isolation options of ADR-R3-6 are set. */
export function buildOptions(
  req: QueryRequest | ChatRequest,
  cfg: SidecarConfig,
  extras: OptionExtras,
): Options {
  const o: Options = {
    model: req.model,
    effort: req.effort,
    maxTurns: Math.min(req.maxTurns, MAX_TURNS_CAP),
    systemPrompt: req.systemPrompt,
    tools: [],
    strictMcpConfig: true,
    permissionMode: "dontAsk",
    settingSources: [],
    cwd: cfg.workDir,
    includePartialMessages: extras.partial,
    // User text is sent as written: no @path file expansion, no slash-command dispatch (CLI >= 2.1.248).
    verbatimPrompts: true,
    abortController: extras.abortController,
    env: childEnv(req, cfg),
  };
  if (extras.session) {
    // A store mirrors the local transcript, so local persistence must stay on.
    if (extras.session.resume !== undefined) o.resume = extras.session.resume;
    else o.sessionId = extras.session.sessionId;
    if (extras.session.store) o.sessionStore = extras.session.store;
  } else {
    o.persistSession = false;
  }
  if ("outputFormat" in req && req.outputFormat) o.outputFormat = req.outputFormat;
  if (extras.mcpServer) {
    o.mcpServers = { coach: extras.mcpServer };
    o.allowedTools = (extras.toolNames ?? []).map((n) => `mcp__coach__${n}`);
  }
  return o;
}
