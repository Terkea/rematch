import { describe, expect, it } from "vitest";
import { SidecarError } from "../src/errors.js";
import { buildOptions, parseChatRequest, parseQueryRequest } from "../src/options.js";
import { testConfig } from "./helpers.js";

const body = {
  model: "claude-sonnet-5-5",
  effort: "medium",
  maxTokens: 16000,
  timeoutMs: 600000,
  maxTurns: 3,
  systemPrompt: "SYS",
  prompt: [{ type: "text", text: "<coaching_input>x</coaching_input>" }],
  outputFormat: { type: "json_schema", schema: { type: "object" } },
};
const CONV = "6f1c3c1e-8d3b-4c1e-9a65-1f2b3c4d5e6f";
const { outputFormat: _of, ...chatBase } = body;
const chatBody = {
  ...chatBase,
  conversationId: CONV,
  resume: false,
  tools: [],
  toolCallback: { url: "http://127.0.0.1:1/v1/tools", token: "turn-token" },
};

describe("buildOptions", () => {
  it("isolation_options_are_exact", () => {
    const cfg = testConfig();
    const abortController = new AbortController();
    const o = buildOptions(parseQueryRequest(body), cfg, { abortController, partial: false });
    const { env, ...rest } = o;
    expect(rest).toEqual({
      model: "claude-sonnet-5-5",
      effort: "medium",
      maxTurns: 3,
      systemPrompt: "SYS",
      tools: [],
      strictMcpConfig: true,
      permissionMode: "dontAsk",
      settingSources: [],
      persistSession: false,
      cwd: cfg.workDir,
      outputFormat: body.outputFormat,
      includePartialMessages: false,
      verbatimPrompts: true,
      abortController,
    });
    for (const k of [
      "resume",
      "sessionId",
      "agents",
      "hooks",
      "fallbackModel",
      "thinking",
      "maxThinkingTokens",
      "mcpServers",
      "allowedTools",
      "sessionStore",
      "enableFileCheckpointing",
    ]) {
      expect(o).not.toHaveProperty(k);
    }
    expect(env).toMatchObject({
      HOME: cfg.configDir,
      CLAUDE_CONFIG_DIR: cfg.configDir,
      CLAUDE_CODE_DISABLE_AUTO_MEMORY: "1",
      CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
      CLAUDE_CODE_MAX_OUTPUT_TOKENS: "16000",
      CLAUDE_AGENT_SDK_CLIENT_APP: "chess-improver-sidecar/0.1",
    });
  });

  it("caps_max_turns_at_10", () => {
    const o = buildOptions(parseQueryRequest({ ...body, maxTurns: 50 }), testConfig(), {
      abortController: new AbortController(),
      partial: false,
    });
    expect(o.maxTurns).toBe(10);
  });

  it("child_env_is_an_allowlist", () => {
    const cfg = testConfig({
      DATABASE_URL: "postgres://secret",
      AI_SESSION_DATABASE_URL: "postgres://ai_sidecar:pw@db/chess",
      CLAUDE_CODE_OAUTH_TOKEN: "oat-SECRET",
      ANTHROPIC_API_KEY: "sk-ant-test-SECRET123",
      SOMETHING_ELSE: "x",
    });
    const o = buildOptions(parseQueryRequest(body), cfg, {
      abortController: new AbortController(),
      partial: false,
    });
    const env = o.env ?? {};
    expect(env.ANTHROPIC_API_KEY).toBe("sk-ant-test-SECRET123");
    expect(env.PATH).toBe("/usr/bin");
    for (const k of [
      "AI_SIDECAR_TOKEN",
      "AI_SESSION_DATABASE_URL",
      "DATABASE_URL",
      "CLAUDE_CODE_OAUTH_TOKEN",
      "SOMETHING_ELSE",
    ]) {
      expect(env).not.toHaveProperty(k);
    }
    expect(Object.keys(env).sort()).toEqual(
      [
        "PATH",
        "HOME",
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_CODE_DISABLE_AUTO_MEMORY",
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
        "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
        "CLAUDE_AGENT_SDK_CLIENT_APP",
        "CLAUDE_CODE_PROJECT_DIR_NAME",
        "ANTHROPIC_API_KEY",
      ].sort(),
    );
  });

  it("oauth_env_carries_only_the_oauth_token", () => {
    const cfg = testConfig({ ANTHROPIC_API_KEY: "", CLAUDE_CODE_OAUTH_TOKEN: "oat-1" });
    const env =
      buildOptions(parseQueryRequest(body), cfg, {
        abortController: new AbortController(),
        partial: false,
      }).env ?? {};
    expect(env.CLAUDE_CODE_OAUTH_TOKEN).toBe("oat-1");
    expect(env).not.toHaveProperty("ANTHROPIC_API_KEY");
  });

  it("cloud_env_carries_provider_variables_only", () => {
    const cfg = testConfig({
      ANTHROPIC_API_KEY: "",
      CLAUDE_CODE_USE_BEDROCK: "1",
      AWS_REGION: "eu-west-1",
      AWS_ACCESS_KEY_ID: "AKIA",
      AWS_SECRET_ACCESS_KEY: "s",
      CLAUDE_CODE_OAUTH_TOKEN: "oat",
    });
    const env =
      buildOptions(parseQueryRequest(body), cfg, {
        abortController: new AbortController(),
        partial: false,
      }).env ?? {};
    expect(env).toMatchObject({
      CLAUDE_CODE_USE_BEDROCK: "1",
      AWS_REGION: "eu-west-1",
      AWS_ACCESS_KEY_ID: "AKIA",
    });
    expect(env).not.toHaveProperty("CLAUDE_CODE_OAUTH_TOKEN");
    expect(env).not.toHaveProperty("ANTHROPIC_API_KEY");
  });

  it("unknown_model_is_rejected", () => {
    for (const model of ["claude-sonnet-5", "sonnet", "claude-opus-5-5"]) {
      try {
        parseQueryRequest({ ...body, model });
        expect.unreachable();
      } catch (e) {
        expect(e).toBeInstanceOf(SidecarError);
        expect((e as SidecarError).status).toBe(400);
        expect((e as SidecarError).type).toBe("invalid_request_error");
      }
    }
    expect(parseQueryRequest({ ...body, model: "claude-haiku-5-5" }).model).toBe(
      "claude-haiku-5-5",
    );
  });

  it("malformed_request_is_400", () => {
    for (const bad of [
      { ...body, effort: "huge" },
      { ...body, prompt: "x" },
      { ...body, maxTokens: -1 },
      { ...body, outputFormat: { type: "text" } },
      null,
    ]) {
      expect(() => parseQueryRequest(bad)).toThrow(SidecarError);
    }
    for (const bad of [
      { ...chatBody, conversationId: "not-a-uuid" },
      { ...chatBody, toolCallback: { url: "ftp://x", token: "t" } },
      { ...chatBody, outputFormat: body.outputFormat },
      { ...chatBody, resume: true }, // a resumable turn must carry its rebuild context
      {
        ...chatBody,
        tools: [{ name: "get_game", description: "d", inputSchema: { type: "object" } }],
        toolCallback: undefined,
      }, // tools need a callback
    ]) {
      expect(() => parseChatRequest(bad)).toThrow(SidecarError);
    }
  });

  it("a_turn_without_tools_needs_no_callback", () => {
    const req = parseChatRequest({ ...chatBody, tools: [], toolCallback: undefined });
    expect(req.tools).toEqual([]);
    expect(req.toolCallback).toBeUndefined();
    // A callback sent anyway is still validated.
    expect(() =>
      parseChatRequest({ ...chatBody, tools: [], toolCallback: { url: "ftp://x", token: "t" } }),
    ).toThrow(SidecarError);
  });

  it("chat_options_allow_only_coach_tools", () => {
    const req = parseChatRequest(chatBody);
    const mcpServer = { type: "sdk", name: "coach", instance: {} } as never;
    const store = { append: async () => {}, load: async () => null };
    const o = buildOptions(req, testConfig(), {
      abortController: new AbortController(),
      partial: true,
      mcpServer,
      toolNames: ["get_position", "get_game"],
      session: { sessionId: CONV, store },
    });
    expect(o.allowedTools).toEqual(["mcp__coach__get_position", "mcp__coach__get_game"]);
    expect(o.mcpServers).toEqual({ coach: mcpServer });
    expect(o.tools).toEqual([]);
    expect(o.includePartialMessages).toBe(true);
    expect(o).not.toHaveProperty("outputFormat");
    // Sessions persist through the store: persistSession must not be false (the SDK throws).
    expect(o).not.toHaveProperty("persistSession");
    expect(o).not.toHaveProperty("enableFileCheckpointing");
    expect(o.sessionId).toBe(CONV);
    expect(o.sessionStore).toBe(store);
    expect(o).not.toHaveProperty("resume");

    const r = buildOptions(req, testConfig(), {
      abortController: new AbortController(),
      partial: true,
      mcpServer,
      toolNames: [],
      session: { resume: CONV, store },
    });
    expect(r.resume).toBe(CONV);
    expect(r).not.toHaveProperty("sessionId");
  });
});
