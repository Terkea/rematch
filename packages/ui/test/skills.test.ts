import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// The project skills (.claude/skills/design-system and chess-ui) cite paths, tokens and component
// names. This keeps them honest: every citation must exist in the code as it is now.
const root = join(process.cwd(), "../..");
const skillsDir = join(root, ".claude/skills");
const skills = ["design-system", "chess-ui"];

const files = skills.flatMap((name) =>
  readdirSync(join(skillsDir, name))
    .filter((f) => f.endsWith(".md"))
    .map((f) => join(skillsDir, name, f)),
);

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules" || entry === ".next" || entry === ".shots") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) walk(full, out);
    else if (/\.(tsx?|css|md)$/.test(entry)) out.push(full);
  }
  return out;
}

const sources = [join(root, "packages/ui/src"), join(root, "apps/web/src")]
  .flatMap((d) => walk(d))
  .map((f) => readFileSync(f, "utf8"))
  .join("\n");
const tokensCss = readFileSync(join(root, "packages/ui/src/styles/tokens.css"), "utf8");

const spans = (text: string) => [...text.matchAll(/`([^`\n]+)`/g)].map((m) => m[1] ?? "");

describe("project skills", () => {
  it("have frontmatter name and description", () => {
    for (const name of skills) {
      const text = readFileSync(join(skillsDir, name, "SKILL.md"), "utf8");
      expect(text).toMatch(new RegExp(`^---\nname: ${name}\ndescription: .+\n---`));
    }
  });

  for (const file of files) {
    const text = readFileSync(file, "utf8");
    const label = file.slice(skillsDir.length + 1);

    it(`${label}: every cited path exists`, () => {
      const missing = spans(text).filter((s) => {
        if (/[<>*\s{}|]/.test(s) || s.startsWith("@") || s.startsWith("pnpm")) return false;
        if (!/^[\w.-]+(\/[\w.()[\]-]+)+\/?$/.test(s) && !/^[\w-]+\.(ts|tsx|css|md)$/.test(s))
          return false;
        if (
          s.startsWith("docs/") ||
          s.startsWith("apps/") ||
          s.startsWith("packages/") ||
          s.startsWith("scripts/")
        )
          return !existsSync(join(root, s));
        return false;
      });
      expect(missing).toEqual([]);
    });

    it(`${label}: every cited --token exists`, () => {
      const missing = spans(text)
        .filter((s) => /^--[a-z0-9-]+$/.test(s))
        .filter((s) => !tokensCss.includes(`${s}:`) && !sources.includes(s));
      expect(missing).toEqual([]);
    });

    it(`${label}: every cited component or function exists`, () => {
      const names = spans(text).flatMap((s) => {
        const m = /^([A-Za-z][A-Za-z0-9]*)(\(.*\))?$/.exec(s);
        const name = m?.[1];
        return name &&
          !/^[BM][0-9]+$/.test(name) &&
          name !== "Infinity" &&
          (/^[A-Z]/.test(name) || m[2] !== undefined)
          ? [name]
          : [];
      });
      // An icon the skill suggests adding may not be exported yet: it must exist in Phosphor.
      const phosphor = (n: string) =>
        n.endsWith("Icon") &&
        existsSync(
          join(
            root,
            "packages/ui/node_modules/@phosphor-icons/react/dist/ssr",
            `${n.slice(0, -4)}.d.ts`,
          ),
        );
      const missing = names.filter(
        (n) => !phosphor(n) && !new RegExp(String.raw`\b${n}(Icon)?\b`).test(sources),
      );
      expect(missing).toEqual([]);
    });
  }
});
