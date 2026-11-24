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

const HIW_START = "<!-- how-it-works:start -->";
const HIW_END = "<!-- how-it-works:end -->";
/** The raw HTML the README's "How it works" uses, which `escapeProse` must not turn into text. */
const DISCLOSURE_TAGS = /&lt;(\/?(?:details|summary))&gt;/g;

/**
 * The site's "How it works" page body from the README, so the two never drift: the text between
 * the how-it-works markers, with links rewritten for the site, headings lifted one level (the page
 * title is the h1), and the mermaid block (GitHub draws it; Starlight cannot) replaced by `diagram`.
 */
export function howItWorksPage(source, diagram) {
  const readme = source.replace(/\r\n/g, "\n");
  const start = readme.indexOf(HIW_START);
  const end = readme.indexOf(HIW_END);
  if (start < 0 || end < start) {
    throw new Error("README.md has no <!-- how-it-works:start --> ... :end --> section");
  }
  const section = readme.slice(start + HIW_START.length, end).trim();
  const withDiagram = section.replace(/^```mermaid\n[\s\S]*?\n```$/m, "@@DIAGRAM@@");
  let fence = null;
  const lifted = rewriteMarkdown(withDiagram, "README.md")
    .split("\n")
    .map((line) => {
      if (/^\s*(`{3,}|~{3,})/.test(line)) fence = fence === null ? line.trim()[0] : null;
      if (fence !== null) return line;
      return line
        .replace(/^(#{3,6}) /, (_m, h) => `${h.slice(1)} `)
        .replace(DISCLOSURE_TAGS, "<$1>");
    })
    .join("\n");
  return `${lifted.replace("@@DIAGRAM@@", diagram.trim())}\n`;
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

/** Markdown inline syntax as plain text: links keep their text, code and emphasis lose marks. */
export function plainText(markdown) {
  return markdown
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/`+([^`]*)`+/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/(^|[^\w*])\*([^*\n]+)\*/g, "$1$2");
}

/** The first sentence of plain text: up to the first `.`, `!` or `?` that ends a word. */
function firstSentence(text) {
  const m = /^.*?[.!?](?=\s+[A-Z"“(]|\s*$)/.exec(text);
  return (m ? m[0] : text).trim();
}

/** The section between the how-it-works markers, with LF line ends. */
function howItWorksSection(source) {
  const readme = source.replace(/\r\n/g, "\n");
  const start = readme.indexOf(HIW_START);
  const end = readme.indexOf(HIW_END);
  if (start < 0 || end < start) {
    throw new Error("README.md has no <!-- how-it-works:start --> ... :end --> section");
  }
  return readme.slice(start + HIW_START.length, end);
}

/**
 * The landing page's "How it works", from the README's section so the two never drift: the
 * overview (the section's first paragraph), then each stage's `<summary>` title and the first
 * sentence of its body (headings and code skipped, a list item's marker dropped), all plain text.
 */
export function howItWorksSummary(source) {
  const section = howItWorksSection(source).trim();
  const paragraph = (lines) => plainText(lines.join(" ")).replace(/\s+/g, " ").trim();
  const overview = paragraph(section.split(/\n\s*\n/)[0]?.split("\n") ?? []);
  const stages = [];
  for (const m of section.matchAll(/<summary>([^<]+)<\/summary>\n([\s\S]*?)<\/details>/g)) {
    const body = (m[2] ?? "").split("\n");
    let fence = false;
    const lines = [];
    for (const line of body) {
      if (/^\s*(`{3,}|~{3,})/.test(line)) fence = !fence;
      if (fence || /^\s*(`{3,}|~{3,})/.test(line)) continue;
      if (lines.length === 0 && (line.trim() === "" || /^#{1,6} /.test(line))) continue;
      if (line.trim() === "" || (lines.length > 0 && /^\s*- /.test(line))) break;
      lines.push(line.replace(/^\s*- /, ""));
    }
    stages.push({ title: (m[1] ?? "").trim(), first: firstSentence(paragraph(lines)) });
  }
  if (stages.length === 0) throw new Error("README.md's how-it-works section has no stage");
  return { overview, stages };
}
