import { readFileSync } from "node:fs";
import type { CoachingInput } from "../../src/input/schema.js";

export type DeepPartial<T> = T extends readonly (infer U)[]
  ? DeepPartial<U>[]
  : T extends object
    ? { [K in keyof T]?: DeepPartial<T[K]> }
    : T;

export function loadFixture(name: string): CoachingInput {
  const url = new URL(`./coaching-input.${name}.json`, import.meta.url);
  return JSON.parse(readFileSync(url, "utf8")) as CoachingInput;
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function merge(base: unknown, over: unknown): unknown {
  if (over === undefined) return base;
  if (Array.isArray(over)) {
    // Arrays merge element-wise onto the base array; extra elements are appended.
    const b = Array.isArray(base) ? base : [];
    return over.map((o, i) => (i < b.length ? merge(b[i], o) : o));
  }
  if (isPlainObject(over) && isPlainObject(base)) {
    const out: Record<string, unknown> = { ...base };
    for (const [k, v] of Object.entries(over)) out[k] = merge(base[k], v);
    return out;
  }
  return over;
}

/** Deep-merges overrides onto the b2-rapid fixture. Arrays merge by index. */
export function makeInput(overrides: DeepPartial<CoachingInput> = {}): CoachingInput {
  return merge(loadFixture("b2-rapid"), overrides) as CoachingInput;
}
