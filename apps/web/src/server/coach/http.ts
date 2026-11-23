import type { PostResult } from "./post.js";

/** The largest request body the chat routes read (a 2000-character question fits many times). */
export const MAX_BODY_BYTES = 16 * 1024;

export const jsonError = (error: string, status: number, headers?: HeadersInit) =>
  Response.json({ error }, { status, ...(headers ? { headers } : {}) });

export const notFound = () => jsonError("not_found", 404);

/**
 * The JSON object body, read with a size cap: 413 `too_large` over `MAX_BODY_BYTES`, 400
 * `invalid_body` when it is not a JSON object or holds a string that is not well-formed Unicode
 * or contains U+0000 (Postgres refuses both).
 */
export async function readJsonBody(req: Request): Promise<Record<string, unknown> | Response> {
  const declared = Number(req.headers.get("content-length") ?? "0");
  if (declared > MAX_BODY_BYTES) return jsonError("too_large", 413);
  if (!req.body) return jsonError("invalid_body", 400);
  const reader = req.body.getReader();
  const parts: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY_BYTES) {
      await reader.cancel();
      return jsonError("too_large", 413);
    }
    parts.push(value);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(parts)));
  } catch {
    return jsonError("invalid_body", 400);
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return jsonError("invalid_body", 400);
  }
  if (!cleanStrings(parsed)) return jsonError("invalid_body", 400);
  return parsed as Record<string, unknown>;
}

const LONE_SURROGATE = /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/;

function cleanStrings(v: unknown): boolean {
  if (typeof v === "string") return !LONE_SURROGATE.test(v) && !v.includes("\u0000");
  if (Array.isArray(v)) return v.every(cleanStrings);
  if (typeof v === "object" && v !== null) {
    return Object.entries(v).every(([k, x]) => cleanStrings(k) && cleanStrings(x));
  }
  return true;
}

/** A post's failure as its response; null for `posted`. */
export function postErrorResponse(r: PostResult): Response | null {
  switch (r.kind) {
    case "posted":
      return null;
    case "not_found":
      return notFound();
    case "not_linked":
      return jsonError("not_linked", 403);
    case "not_verified":
      return jsonError("not_verified", 403);
    case "busy":
      return jsonError("busy", 409);
    case "empty":
    case "too_long":
      return jsonError(r.kind, 400);
    case "rate_limited":
      return rateLimited(r.retryAt);
  }
}

export function rateLimited(retryAt: Date): Response {
  const seconds = Math.max(1, Math.ceil((retryAt.getTime() - Date.now()) / 1000));
  return Response.json(
    { error: "rate_limited", retryAt: retryAt.toISOString() },
    { status: 429, headers: { "retry-after": String(seconds) } },
  );
}
