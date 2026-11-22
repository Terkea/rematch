// Links and keys the report shares between its server page and its client sections (a plain
// module, so a server component can call them).

import type { TimeClass } from "../../../../../server/games/types";

type Scope = { accountId: string; timeClass: TimeClass };

const scopeQuery = (scope: Scope) =>
  new URLSearchParams({ account: scope.accountId, tc: scope.timeClass }).toString();

/** The profile series a ranked weakness reads (`W3` with variant `shallow` is `W3.shallow`). */
export const weaknessSeriesKey = (w: { id: string; variant: string | null }) =>
  w.variant ? `${w.id}.${w.variant}` : w.id;

export const weaknessHref = (seriesKey: string, scope: Scope) =>
  `/app/weaknesses/${encodeURIComponent(seriesKey)}?${scopeQuery(scope)}`;

export const areaHref = (area: string, scope: Scope) =>
  `/app/areas/${encodeURIComponent(area)}?${scopeQuery(scope)}`;

/** The Strengths page of the scope (`/app/strengths`, the same `select_strengths` rules). */
export const strengthsHref = (scope: Scope) => `/app/strengths?${scopeQuery(scope)}`;

/**
 * A computed strength's label in the page's words: Rust writes an area strength as
 * "Strong area: <area id>"; the list already names the area beside it, so it reads "A strong
 * area". Any other label ("Few errors: …") as it is.
 */
export function strengthLabel(s: { label: string }): string {
  return s.label.startsWith("Strong area: ") ? "A strong area" : s.label;
}

type GameRef = { gameId?: unknown };

/** Every platform game id a report cites: in its coach segments and its example positions. */
export function reportGameIds(payload: {
  sections: Record<string, unknown>;
  computed: {
    weaknesses: { exemplars: GameRef[] }[];
    openings: { examples: GameRef[] }[];
    strengths: { exemplars: GameRef[] }[];
  };
}): string[] {
  const out = new Set<string>();
  const add = (v: unknown) => {
    if (typeof v === "string" && v.length > 0) out.add(v);
  };
  // Segments sit at any depth of a section's content; walk it for `{ kind, gameId }` objects.
  const walk = (v: unknown) => {
    if (Array.isArray(v)) {
      for (const x of v) walk(x);
    } else if (v && typeof v === "object") {
      const o = v as Record<string, unknown>;
      if (typeof o.kind === "string" && "gameId" in o) add(o.gameId);
      for (const x of Object.values(o)) walk(x);
    }
  };
  walk(payload.sections);
  for (const w of payload.computed.weaknesses) for (const e of w.exemplars) add(e.gameId);
  for (const o of payload.computed.openings) for (const e of o.examples) add(e.gameId);
  for (const s of payload.computed.strengths) for (const e of s.exemplars) add(e.gameId);
  return [...out];
}

/** A segment's review link through the platform-to-`games.id` map, or none. */
export function reviewHref(
  s: { kind: string; gameId?: unknown; ply?: unknown },
  gameIds: Record<string, string>,
): string | null {
  if (s.kind !== "move" && s.kind !== "best" && s.kind !== "pv") return null;
  if (typeof s.gameId !== "string" || typeof s.ply !== "number") return null;
  const id = Object.hasOwn(gameIds, s.gameId) ? gameIds[s.gameId] : undefined;
  return id ? `/app/games/${encodeURIComponent(id)}?ply=${s.ply}` : null;
}

/** Rust `change_key`: `"{seriesKey}@{onsetGameId}"`, the key a change note is written under. */
export const changeKey = (c: { seriesKey: string; onsetGameId: string }) =>
  `${c.seriesKey}@${c.onsetGameId}`;
