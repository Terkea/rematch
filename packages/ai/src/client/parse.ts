import type Anthropic from "@anthropic-ai/sdk";
import type { z } from "zod";
import type { ParseError, ParseIssue } from "./types.js";

export interface ParseOutcome {
  parsed: unknown | null;
  parseError: ParseError | null;
  parseIssues: ParseIssue[] | null;
}

function firstText(content: Anthropic.ContentBlock[]): string | null {
  for (const b of content) if (b.type === "text") return b.text;
  return null;
}

/**
 * Shared by the real adapter, batch results and the fake client. A null `parsed` always carries a
 * reason. Issues carry zod paths and codes only, never content.
 */
export function parseModelOutput(
  content: Anthropic.ContentBlock[],
  stopReason: string | null,
  schema: z.ZodType | undefined,
): ParseOutcome {
  const fail = (parseError: ParseError, parseIssues: ParseIssue[] | null = null): ParseOutcome => ({
    parsed: null,
    parseError,
    parseIssues,
  });
  if (stopReason === "refusal") return fail("refusal");
  if (stopReason === "max_tokens") return fail("truncated");
  if (!schema) return fail("no_schema");
  const text = firstText(content);
  if (text === null) return fail("no_text");
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch {
    return fail("invalid_json");
  }
  const r = schema.safeParse(json);
  if (!r.success) {
    return fail(
      "schema_mismatch",
      r.error.issues.map((i) => ({ path: i.path.map(String).join("."), code: i.code })),
    );
  }
  return { parsed: r.data, parseError: null, parseIssues: null };
}
