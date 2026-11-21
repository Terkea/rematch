/**
 * Batch custom_id is `<schemaId>__v<version>__<ref>`. The API only allows [A-Za-z0-9_-]{1,64},
 * so the `@` and `:` separators are not usable. schemaId has no underscores and version is
 * digits, so the first two `__` delimit unambiguously; `ref` may contain anything allowed.
 */
export const CUSTOM_ID_MAX = 64;

const SCHEMA_ID = /^[A-Za-z0-9-]+$/;
const REF = /^[A-Za-z0-9_-]+$/;

export interface CustomIdParts {
  schemaId: string;
  version: number;
  ref: string;
}

export function encodeCustomId(p: CustomIdParts): string {
  if (!SCHEMA_ID.test(p.schemaId)) throw new Error("schemaId must match [A-Za-z0-9-]+");
  if (!Number.isInteger(p.version) || p.version < 0) throw new Error("version must be an integer");
  if (!REF.test(p.ref)) throw new Error("ref must match [A-Za-z0-9_-]+");
  const id = `${p.schemaId}__v${p.version}__${p.ref}`;
  if (id.length > CUSTOM_ID_MAX) throw new Error(`custom_id exceeds ${CUSTOM_ID_MAX} characters`);
  return id;
}

export function decodeCustomId(id: string): CustomIdParts | null {
  const m = /^([A-Za-z0-9-]+)__v(\d+)__([A-Za-z0-9_-]+)$/.exec(id);
  if (!m || id.length > CUSTOM_ID_MAX) return null;
  return { schemaId: m[1] as string, version: Number(m[2]), ref: m[3] as string };
}
