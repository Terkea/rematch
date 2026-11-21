import { type ZodRawShape, type ZodType, z } from "zod";
import { SchemaUnsupported } from "./errors.js";

/**
 * Converts the JSON Schema subset that Rust's tool definitions (T22 `ToolDef.input_schema`) use
 * into the zod raw shape `tool()` needs. Anything outside the subset throws `SchemaUnsupported`
 * naming its path, so an unexpected schema fails loudly instead of being loosened.
 */
type Json = Record<string, unknown>;

const isObj = (v: unknown): v is Json => typeof v === "object" && v !== null && !Array.isArray(v);

function onlyKeys(s: Json, allowed: string[], path: string): void {
  if (Object.keys(s).some((k) => !allowed.includes(k))) throw new SchemaUnsupported(path);
}

function num(v: unknown, path: string): number | undefined {
  if (v === undefined) return undefined;
  if (typeof v !== "number" || !Number.isFinite(v)) throw new SchemaUnsupported(path);
  return v;
}

function withDescription<T extends ZodType>(t: T, s: Json, path: string): T {
  if (s.description === undefined) return t;
  if (typeof s.description !== "string") throw new SchemaUnsupported(`${path}.description`);
  return t.describe(s.description) as T;
}

function property(s: unknown, path: string): ZodType {
  if (!isObj(s)) throw new SchemaUnsupported(path);
  switch (s.type) {
    case "string": {
      onlyKeys(s, ["type", "description", "enum"], path);
      if (s.enum === undefined) return withDescription(z.string(), s, path);
      const e = s.enum;
      if (!Array.isArray(e) || e.length === 0 || !e.every((x) => typeof x === "string")) {
        throw new SchemaUnsupported(`${path}.enum`);
      }
      return withDescription(z.enum(e as [string, ...string[]]), s, path);
    }
    case "integer": {
      onlyKeys(s, ["type", "description", "minimum", "maximum"], path);
      let t = z.number().int();
      const min = num(s.minimum, `${path}.minimum`);
      const max = num(s.maximum, `${path}.maximum`);
      if (min !== undefined) t = t.min(min);
      if (max !== undefined) t = t.max(max);
      return withDescription(t, s, path);
    }
    case "number":
      onlyKeys(s, ["type", "description"], path);
      return withDescription(z.number(), s, path);
    case "boolean":
      onlyKeys(s, ["type", "description"], path);
      return withDescription(z.boolean(), s, path);
    case "array": {
      onlyKeys(s, ["type", "description", "items"], path);
      const items = s.items;
      if (!isObj(items) || items.type !== "string") throw new SchemaUnsupported(`${path}.items`);
      onlyKeys(items, ["type", "description"], `${path}.items`);
      return withDescription(z.array(z.string()), s, path);
    }
    default:
      throw new SchemaUnsupported(path);
  }
}

export function jsonSchemaToZodShape(schema: unknown): ZodRawShape {
  const path = "$";
  if (!isObj(schema) || schema.type !== "object") throw new SchemaUnsupported(path);
  for (const k of Object.keys(schema)) {
    if (!["type", "properties", "required", "additionalProperties", "description"].includes(k)) {
      throw new SchemaUnsupported(`${path}.${k}`);
    }
  }
  if (schema.additionalProperties !== undefined && schema.additionalProperties !== false) {
    throw new SchemaUnsupported(`${path}.additionalProperties`);
  }
  const props = schema.properties ?? {};
  if (!isObj(props)) throw new SchemaUnsupported(`${path}.properties`);
  const types = Object.entries(props).map(
    ([name, sub]) => [name, property(sub, `${path}.properties.${name}`)] as const,
  );
  const required = schema.required ?? [];
  if (
    !Array.isArray(required) ||
    !required.every((r) => typeof r === "string" && Object.hasOwn(props, r))
  ) {
    throw new SchemaUnsupported(`${path}.required`);
  }
  return Object.fromEntries(
    types.map(([name, t]) => [name, required.includes(name) ? t : t.optional()]),
  ) as ZodRawShape;
}
