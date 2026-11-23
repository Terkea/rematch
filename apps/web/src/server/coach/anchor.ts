import { SECTION_KEYS, type SectionKey } from "../coaching/types.js";
import type { TimeClass } from "../games/types.js";

/**
 * The context a conversation or a message is opened from: the serde form of Rust
 * `chat::anchor::ChatAnchor` (tag `kind` in snake_case, camelCase fields, unknown fields denied).
 * The worker binds it to the conversation's account and ignores a foreign one (R3 Task 25).
 */
export type ChatAnchor =
  | { kind: "report_section"; reportId: string; section: SectionKey }
  | { kind: "weakness"; weaknessId: string }
  | { kind: "position"; gameId: string; ply: number }
  | { kind: "move"; gameId: string; ply: number }
  | { kind: "opening_line"; lineKey: string }
  | { kind: "game"; gameId: string };

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** The longest id or key an anchor may carry. */
const MAX_ID = 200;
/** The longest encoded anchor `decodeAnchor` reads. */
const MAX_ENCODED = 1024;
/** Plies a game may have (Rust takes any u32; the web bounds it). */
const MAX_PLY = 1000;

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

// Printable text only: no control characters (Postgres also refuses U+0000 in jsonb).
// biome-ignore lint/suspicious/noControlCharactersInRegex: the point is to refuse them
const CONTROL = /[\u0000-\u001f\u007f]/;
const isId = (v: unknown): v is string =>
  typeof v === "string" && v.length > 0 && v.length <= MAX_ID && !CONTROL.test(v);
const isPly = (v: unknown): v is number =>
  typeof v === "number" && Number.isInteger(v) && v >= 1 && v <= MAX_PLY;

function exactKeys(v: Record<string, unknown>, keys: string[]): boolean {
  const own = Object.keys(v);
  return own.length === keys.length && keys.every((k) => Object.hasOwn(v, k));
}

/** A strict parse of an untrusted anchor; anything else (extra keys, wrong types) is null. */
export function parseAnchor(raw: unknown): ChatAnchor | null {
  if (!isRecord(raw)) return null;
  switch (raw.kind) {
    case "report_section": {
      const { reportId, section } = raw;
      if (!exactKeys(raw, ["kind", "reportId", "section"])) return null;
      if (typeof reportId !== "string" || !UUID.test(reportId)) return null;
      if (!SECTION_KEYS.includes(section as SectionKey)) return null;
      return {
        kind: "report_section",
        reportId: reportId.toLowerCase(),
        section: section as SectionKey,
      };
    }
    case "weakness":
      if (!exactKeys(raw, ["kind", "weaknessId"]) || !isId(raw.weaknessId)) return null;
      return { kind: "weakness", weaknessId: raw.weaknessId };
    case "position":
    case "move":
      if (!exactKeys(raw, ["kind", "gameId", "ply"]) || !isId(raw.gameId) || !isPly(raw.ply)) {
        return null;
      }
      return { kind: raw.kind, gameId: raw.gameId, ply: raw.ply };
    case "opening_line":
      if (!exactKeys(raw, ["kind", "lineKey"]) || !isId(raw.lineKey)) return null;
      return { kind: "opening_line", lineKey: raw.lineKey };
    case "game":
      if (!exactKeys(raw, ["kind", "gameId"]) || !isId(raw.gameId)) return null;
      return { kind: "game", gameId: raw.gameId };
    default:
      return null;
  }
}

// base64url without Buffer, so links are built the same in the browser and on the server.
function toBase64Url(text: string): string {
  let bin = "";
  for (const byte of new TextEncoder().encode(text)) bin += String.fromCharCode(byte);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(s: string): string {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

/** The anchor as base64url JSON, for a URL parameter. */
export function encodeAnchor(a: ChatAnchor): string {
  return toBase64Url(JSON.stringify(a));
}

/** `encodeAnchor`'s inverse; a malformed, oversized or invalid value is null. */
export function decodeAnchor(s: string): ChatAnchor | null {
  if (s.length === 0 || s.length > MAX_ENCODED || !/^[A-Za-z0-9_-]+$/.test(s)) return null;
  try {
    return parseAnchor(JSON.parse(fromBase64Url(s)));
  } catch {
    return null;
  }
}

/**
 * A new chat in the scope, opened from `anchor`. A scope read for one colour (ADR 0082) keeps
 * it as `color`, so the chat is about that colour's games (`colourQuestion`).
 */
export function coachHref(
  anchor: ChatAnchor | null,
  scope: { accountId: string; timeClass: TimeClass; side?: "both" | "white" | "black" },
): string {
  const q = new URLSearchParams({ account: scope.accountId, tc: scope.timeClass });
  if (anchor) q.set("anchor", encodeAnchor(anchor));
  if (scope.side === "white" || scope.side === "black") q.set("color", scope.side);
  return `/app/coach/new?${q.toString()}`;
}

/** The colour of a `color` parameter or body field; null for Both or anything else. */
export function askedColour(v: unknown): "white" | "black" | null {
  return v === "white" || v === "black" ? v : null;
}

/**
 * The question as the coach receives it. The chat context holds both colours' numbers
 * (`byColor`, ADR 0085); a chat opened from one colour's page says which colour it is about, in
 * the player's own visible words, so the answer uses that colour's numbers.
 */
export function colourQuestion(question: string, color: unknown): string {
  const c = askedColour(color);
  if (!c) return question;
  return `About my games with ${c === "white" ? "White" : "Black"}: ${question}`;
}

/** Rust `chat::summary::fallback_title`: the title when the model gave none. */
export function fallbackTitle(anchor: ChatAnchor | null): string {
  switch (anchor?.kind) {
    case "report_section":
      return "Chat about your report";
    case "weakness":
      return "Chat about a weakness";
    case "position":
      return "Chat about a position";
    case "move":
      return "Chat about a move";
    case "opening_line":
      return "Chat about an opening";
    case "game":
      return "Chat about a game";
    default:
      return "Chat with your coach";
  }
}
