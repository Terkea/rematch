import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { howItWorksSummary, plainText } from "../scripts/content-lib.mjs";

// The landing page's "How it works" is generated from the README's section (one source): its
// overview paragraph, then each stage's <summary> title and the first sentence of its body.

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const README = readFileSync(join(ROOT, "README.md"), "utf8").replace(/\r\n/g, "\n");
const LANDING = readFileSync(join(ROOT, "site", "src", "pages", "index.astro"), "utf8");

const section = (md: string) =>
  md.slice(md.indexOf("<!-- how-it-works:start -->"), md.indexOf("<!-- how-it-works:end -->"));

describe("howItWorksSummary", () => {
  const md = [
    "<!-- how-it-works:start -->",
    "",
    "A game becomes advice: [Stockfish](https://stockfishchess.org) checks **every** move.",
    "Then `more`.",
    "",
    "```mermaid",
    "flowchart LR",
    "```",
    "",
    "<details>",
    "<summary>Import: chess.com only</summary>",
    "",
    "- Source: The archives at `https://api.chess.com/pub`. A second sentence, 0.15 long.",
    "",
    "</details>",
    "",
    "<details>",
    "<summary>Engine: Stockfish 19</summary>",
    "",
    "#### The engine",
    "",
    "Every position at 300,000 nodes, a ΔW of 0.15 included. Then more ([ADR](x.md)).",
    "",
    "</details>",
    "<!-- how-it-works:end -->",
  ].join("\n");

  it("takes the overview paragraph and each stage's title and first sentence, as plain text", () => {
    expect(howItWorksSummary(md)).toEqual({
      overview: "A game becomes advice: Stockfish checks every move. Then more.",
      stages: [
        {
          title: "Import: chess.com only",
          first: "Source: The archives at https://api.chess.com/pub.",
        },
        {
          title: "Engine: Stockfish 19",
          first: "Every position at 300,000 nodes, a ΔW of 0.15 included.",
        },
      ],
    });
  });

  it("fails loudly without the markers or the stages", () => {
    expect(() => howItWorksSummary("# Rematch")).toThrow(/how-it-works/);
    expect(() =>
      howItWorksSummary("<!-- how-it-works:start -->\n\nText.\n<!-- how-it-works:end -->"),
    ).toThrow(/stage/);
  });
});

describe("the landing page's How it works", () => {
  const summary = howItWorksSummary(README);
  const readmeText = plainText(section(README)).replace(/\s+/g, " ");

  it("is a subset of the README section: the overview, every stage title and first sentence", () => {
    expect(summary.stages.length).toBeGreaterThanOrEqual(10);
    expect(readmeText).toContain(summary.overview);
    for (const s of summary.stages) {
      expect(readmeText).toContain(s.title);
      expect(readmeText).toContain(s.first);
      expect(s.first).toMatch(/[.!?]$/);
    }
  });

  it("renders only the generated summary, with the shared diagram and the link to the docs", () => {
    expect(LANDING).toContain('from "../data/generated/how-it-works.json"');
    expect(LANDING).not.toMatch(/const stages = \[/);
    expect(LANDING).toContain("<Diagram />");
    expect(LANDING).toContain('href={href("docs/how-it-works/")}');
    expect(LANDING).toContain("Every stage in full, with the algorithms");
  });
});
