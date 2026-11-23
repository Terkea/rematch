import manifest from "../../../../algorithm/manifest.json";

// The algorithm this build runs (ADR 0105): the committed algorithm/manifest.json, generated
// from the Rust and TS constants by `cli algo manifest --write`. Pure, so client components can
// link a row's algorithm to its changelog entry. Writers use `server/algorithm.ts`.

export const ALGORITHM_ID: string = manifest.id;
export const ALGORITHM_VERSION: string = manifest.version;
export const ALGORITHM_MANIFEST = manifest;

const CHANGELOG = "https://github.com/Terkea/rematch/blob/main/algorithm/CHANGELOG.md";

/** GitHub's anchor for the heading `## <id>`: lower case, only letters, digits, `-` and `_`. */
export const changelogAnchor = (id: string): string =>
  id
    .toLowerCase()
    .replace(/[^a-z0-9_ -]/g, "")
    .replace(/ /g, "-");

/** The algorithm behind a row, for "Why this?": its id, its version and its changelog entry. */
export type AlgorithmLink =
  | { id: string; version: string; href: string }
  | { id: null; version: null; href: null };

/** `null` is a row written before versioning. */
export function algorithmLink(id: string | null | undefined): AlgorithmLink {
  if (!id) return { id: null, version: null, href: null };
  const version = /^algo-([0-9]+\.[0-9]+\.[0-9]+)\+/.exec(id)?.[1] ?? id;
  return { id, version, href: `${CHANGELOG}#${changelogAnchor(id)}` };
}
