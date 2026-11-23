// Pure checks the maintenance sweeps run (ADR 0099). No dependencies, no network.

/**
 * The advisory ignores in backend/deny.toml. Each entry must end with `# expires: YYYY-MM-DD`,
 * the day after which the exception is stale: the weekly sweep fails from the next day on.
 * Returns {entries: [{id, expires}], problems: [string]}.
 */
export function advisoryIgnores(denyToml, today) {
  const lines = denyToml.replaceAll("\r\n", "\n").split("\n");
  const start = lines.findIndex((l) => /^\[advisories\]\s*$/.test(l));
  const entries = [];
  const problems = [];
  if (start < 0) return { entries, problems: ["deny.toml has no [advisories] table"] };
  let inIgnore = false;
  for (const line of lines.slice(start + 1)) {
    if (/^\[/.test(line)) break;
    if (/^ignore\s*=\s*\[/.test(line)) inIgnore = true;
    if (!inIgnore) continue;
    const id = /id\s*=\s*"([^"]+)"/.exec(line)?.[1];
    if (id) {
      const expires = /#\s*expires:\s*(\d{4}-\d{2}-\d{2})\b/.exec(line)?.[1];
      entries.push({ id, expires: expires ?? null });
      if (!expires) {
        problems.push(`${id}: no "# expires: YYYY-MM-DD" comment on its line`);
      } else if (today > expires) {
        problems.push(`${id}: the ignore expired on ${expires}; fix the advisory and remove it`);
      }
    }
    if (/\]\s*(#.*)?$/.test(line) && !/\[\s*$/.test(line) && !id) inIgnore = false;
  }
  return { entries, problems };
}

/** The ADR ids a text mentions: "ADR 0095", "ADRs 0030, 0031", "ADR-R2-12", "ADR-WEB-7". */
export function adrReferences(text) {
  const ids = new Set();
  for (const m of text.matchAll(/\bADRs?[ -](\d{4}(?:(?:,\s*|\s+and\s+)\d{4})*)/g)) {
    for (const n of m[1].match(/\d{4}/g) ?? []) ids.add(n);
  }
  for (const m of text.matchAll(/\bADR-((?:R\d+|WEB|FIX-[A-Z]+)-\d+)\b/g)) ids.add(`ADR-${m[1]}`);
  return [...ids];
}

/** The id of an ADR file: "0095" from "0095-open-source.md", "ADR-R2-12" from its file. */
export function adrIdOf(file) {
  return (
    /^(\d{4})-/.exec(file)?.[1] ?? /^(ADR-(?:R\d+|WEB|FIX-[A-Z]+)-\d+)-/.exec(file)?.[1] ?? null
  );
}

/**
 * The decision log's index against its files: every ADR file is linked from the README, and
 * every link there points at a file that exists.
 */
export function adrIndexProblems(readme, files) {
  const problems = [];
  const linked = [...readme.matchAll(/\]\(([^)#]+\.md)\)/g)].map((m) => m[1]);
  const fileSet = new Set(files);
  for (const l of linked) {
    if (!fileSet.has(l)) problems.push(`docs/decisions/README.md links ${l}, which does not exist`);
  }
  const linkedSet = new Set(linked);
  for (const f of files) {
    if (f !== "README.md" && !linkedSet.has(f)) {
      problems.push(`docs/decisions/${f} is not listed in docs/decisions/README.md`);
    }
  }
  return problems;
}
