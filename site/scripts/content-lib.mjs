// Pure helpers for sync-content.mjs, kept separate so test/content-lib.test.mjs can cover them.
import { posix } from "node:path";

export const REPO = "Terkea/rematch";
export const BASE = "/rematch";
export const GITHUB_BLOB = `https://github.com/${REPO}/blob/main`;

/** The page slug for a markdown file name: lower case, no extension. */
export function slugOf(file) {
  return file.replace(/\.md$/, "").toLowerCase();
}

/** The first level-one heading, or null. */
export function titleOf(markdown) {
  const m = /^# (.+)$/m.exec(markdown);
  return m?.[1]?.trim() ?? null;
}

const COLLECTIONS = { "docs/decisions": "decisions", "docs/research": "research" };

/**
 * Where a relative link in `fromRepoPath` should point on the site: a generated page when the
 * target is a decision or research note, otherwise the file on GitHub. External links, anchors
 * and mail links are returned unchanged.
 */
export function rewriteLink(target, fromRepoPath) {
  if (/^(?:[a-z][a-z0-9+.-]*:|#)/i.test(target)) return target;
  const [pathPart = "", ...rest] = target.split("#");
  const anchor = rest.length ? `#${rest.join("#")}` : "";
  if (pathPart === "") return target;
  const resolved = pathPart.startsWith("/")
    ? posix.normalize(pathPart.slice(1))
    : posix.normalize(posix.join(posix.dirname(fromRepoPath), pathPart));
  if (resolved.startsWith("..")) return target;
  const dir = posix.dirname(resolved);
  const route = COLLECTIONS[dir];
  if (route && resolved.endsWith(".md")) {
    const file = posix.basename(resolved);
    if (file === "README.md") return `${BASE}/docs/${route}/${anchor}`;
    return `${BASE}/docs/${route}/${slugOf(file)}/${anchor}`;
  }
  if (COLLECTIONS[resolved.replace(/\/$/, "")]) {
    return `${BASE}/docs/${COLLECTIONS[resolved.replace(/\/$/, "")]}/${anchor}`;
  }
  return `${GITHUB_BLOB}/${resolved}${anchor}`;
}

/** Escapes `<word>` placeholders in prose (outside code), so they are not read as HTML tags. */
function escapeProse(segment) {
  return segment.replace(/<(\/?[A-Za-z_][\w-]*)>/g, "&lt;$1&gt;");
}

function rewriteLine(line, fromRepoPath) {
  // Reference-style definitions: [label]: target
  const def = /^(\s*\[[^\]]+\]:\s*)(\S+)(.*)$/.exec(line);
  if (def) return `${def[1]}${rewriteLink(def[2] ?? "", fromRepoPath)}${def[3] ?? ""}`;
  // Split on inline code spans; only the prose between them is touched.
  return line
    .split(/(`+[^`]*`+)/)
    .map((part, i) => {
      if (i % 2 === 1) return part;
      const linked = part.replace(
        /\]\(([^)\s]+)(\s+"[^"]*")?\)/g,
        (_m, t, title) => `](${rewriteLink(t, fromRepoPath)}${title ?? ""})`,
      );
      return escapeProse(linked);
    })
    .join("");
}

/** Rewrites links and escapes placeholders in a markdown document, leaving fenced code alone. */
export function rewriteMarkdown(markdown, fromRepoPath) {
  let fence = null;
  return markdown
    .split("\n")
    .map((line) => {
      const f = /^\s*(`{3,}|~{3,})/.exec(line);
      if (f) {
        const mark = f[1]?.[0] ?? "`";
        if (fence === null) fence = mark;
        else if (fence === mark) fence = null;
        return line;
      }
      return fence === null ? rewriteLine(line, fromRepoPath) : line;
    })
    .join("\n");
}
