// Reference tokens (ADR-AI-8): narrative text never states chess facts directly; it points at them.
import { stripInvisibles } from "./normalize.js";

export const NUM_FORMATS = ["int", "pct0", "pct1", "dec1", "dec2", "signed1"] as const;
export type NumFormat = (typeof NUM_FORMATS)[number];

export const TOKEN_PLACEHOLDER = "§";

export type TokenBody =
  | { kind: "move"; gameId: string; ply: number }
  | { kind: "best"; gameId: string; ply: number }
  | { kind: "pv"; gameId: string; ply: number; side: "best" | "refutation"; n: number }
  | { kind: "line"; lineKey: string; n: number | null }
  | { kind: "num"; path: string; fmt: NumFormat | null }
  | { kind: "date"; path: string }
  | { kind: "sq"; square: string };
export type Token = TokenBody & { raw: string; index: number };

export interface TokenSyntaxError {
  message: string;
  raw: string;
  index: number;
}

const ID_RE = /^[A-Za-z0-9_.-]+$/;
const PLY_RE = /^[1-9]\d{0,3}$/;
const N_RE = /^[1-9]\d{0,2}$/;
const PATH_RE = /^[A-Za-z0-9_#-]+(?:\.[A-Za-z0-9_#-]+)*$/;
const LINE_RE = /^[A-Za-z0-9_.:-]+$/;
const SQUARE_RE = /^[a-h][1-8]$/;

/** The text tokens are parsed from: invisible characters stripped, NFKC applied. Renderers must use the same form. */
export function canonicalForTokens(text: string): string {
  return stripInvisibles(stripInvisibles(text).normalize("NFKC"));
}

function parseInner(inner: string): TokenBody | string {
  const parts = inner.split(":");
  const kind = parts[0];
  switch (kind) {
    case "move":
    case "best": {
      const [, gameId, ply] = parts;
      if (parts.length !== 3 || !gameId || !ID_RE.test(gameId) || !ply || !PLY_RE.test(ply)) {
        return `${kind} token needs {{${kind}:<gameId>:<ply>}}`;
      }
      return { kind, gameId, ply: Number(ply) };
    }
    case "pv": {
      const [, gameId, ply, side, n] = parts;
      if (
        parts.length !== 5 ||
        !gameId ||
        !ID_RE.test(gameId) ||
        !ply ||
        !PLY_RE.test(ply) ||
        (side !== "best" && side !== "refutation") ||
        !n ||
        !N_RE.test(n)
      ) {
        return "pv token needs {{pv:<gameId>:<ply>:best|refutation:<n>}}";
      }
      return { kind: "pv", gameId, ply: Number(ply), side, n: Number(n) };
    }
    case "line": {
      if (parts.length < 2) return "line token needs {{line:<lineKey>[:<n>]}}";
      let n: number | null = null;
      let keyParts = parts.slice(1);
      const last = keyParts[keyParts.length - 1];
      if (keyParts.length >= 2 && last !== undefined && N_RE.test(last)) {
        n = Number(last);
        keyParts = keyParts.slice(0, -1);
      }
      const lineKey = keyParts.join(":");
      if (!lineKey || !LINE_RE.test(lineKey)) return "line token has an invalid line key";
      return { kind: "line", lineKey, n };
    }
    case "num": {
      const rest = inner.slice(4);
      const [path, fmt, ...extra] = rest.split("|");
      if (!path || !PATH_RE.test(path) || extra.length > 0)
        return "num token needs {{num:<path>[|<fmt>]}}";
      if (fmt !== undefined && !(NUM_FORMATS as readonly string[]).includes(fmt)) {
        return `unknown number format "${fmt}"`;
      }
      return { kind: "num", path, fmt: (fmt as NumFormat | undefined) ?? null };
    }
    case "date": {
      const path = inner.slice(5);
      if (!path || !PATH_RE.test(path)) return "date token needs {{date:<path>}}";
      return { kind: "date", path };
    }
    case "sq": {
      const sq = parts[1];
      if (parts.length !== 2 || !sq || !SQUARE_RE.test(sq))
        return "sq token needs a square name a1-h8";
      return { kind: "sq", square: sq };
    }
    default:
      return `unknown token kind "${kind ?? ""}"`;
  }
}

export function parseTokens(text: string): {
  tokens: Token[];
  stripped: string;
  errors: TokenSyntaxError[];
} {
  const src = canonicalForTokens(text);
  const tokens: Token[] = [];
  const errors: TokenSyntaxError[] = [];
  const stripped = src.replace(/\{\{([^{}]*)\}\}/g, (raw: string, inner: string, index: number) => {
    const r = parseInner(inner);
    if (typeof r === "string") errors.push({ message: r, raw, index });
    else tokens.push({ ...r, raw, index } as Token);
    return TOKEN_PLACEHOLDER;
  });
  for (const m of stripped.matchAll(/[{}]+/g)) {
    errors.push({ message: "stray or unterminated brace", raw: m[0], index: m.index ?? 0 });
  }
  return { tokens, stripped, errors };
}
