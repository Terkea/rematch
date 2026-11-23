import { describe, expect, it } from "vitest";
import {
  howItWorksPage,
  rewriteLink,
  rewriteMarkdown,
  slugOf,
  titleOf,
} from "../scripts/content-lib.mjs";
import { tokensToPlainCss } from "../scripts/sync-content.mjs";

const FROM = "docs/decisions/0090-missed-moments-trainer.md";

describe("rewriteLink", () => {
  it("sends a sibling decision to its generated page, keeping the anchor", () => {
    expect(rewriteLink("0091-weekly-focus-and-goal.md#decision", FROM)).toBe(
      "/rematch/docs/decisions/0091-weekly-focus-and-goal/#decision",
    );
  });
  it("lower-cases series decisions", () => {
    expect(rewriteLink("ADR-WEB-7-design-tokens.md", FROM)).toBe(
      "/rematch/docs/decisions/adr-web-7-design-tokens/",
    );
  });
  it("sends research notes to the research pages and the README to the index", () => {
    expect(rewriteLink("../research/r1-parity.md", FROM)).toBe("/rematch/docs/research/r1-parity/");
    expect(rewriteLink("README.md", FROM)).toBe("/rematch/docs/decisions/");
  });
  it("sends any other repository file to GitHub", () => {
    expect(rewriteLink("../../packages/ui/DESIGN.md", FROM)).toBe(
      "https://github.com/Terkea/rematch/blob/main/packages/ui/DESIGN.md",
    );
  });
  it("leaves external links and anchors alone", () => {
    expect(rewriteLink("https://lichess.org", FROM)).toBe("https://lichess.org");
    expect(rewriteLink("#context", FROM)).toBe("#context");
  });
});

describe("rewriteMarkdown", () => {
  it("rewrites links in prose but not in code", () => {
    const md = [
      "See [focus](0091-weekly-focus-and-goal.md) and `[x](0091.md)`.",
      "```",
      "[y](0091-weekly-focus-and-goal.md)",
      "```",
    ].join("\n");
    const out = rewriteMarkdown(md, FROM);
    expect(out).toContain("](/rematch/docs/decisions/0091-weekly-focus-and-goal/)");
    expect(out).toContain("`[x](0091.md)`");
    expect(out).toContain("[y](0091-weekly-focus-and-goal.md)");
  });
  it("escapes placeholders outside code so they are not HTML tags", () => {
    expect(rewriteMarkdown("available <when> now, `<anchor>`", FROM)).toBe(
      "available &lt;when&gt; now, `<anchor>`",
    );
  });
});

describe("howItWorksPage", () => {
  const readme = [
    "# Rematch",
    "## How it works",
    "<!-- how-it-works:start -->",
    "```mermaid",
    "flowchart LR",
    "  a --> b",
    "```",
    "",
    "See [ADR 0090](docs/decisions/0090-missed-moments-trainer.md) and [`score.rs`](backend/crates/chess-core/src/score.rs).",
    "",
    "### One blunder",
    "",
    "<details>",
    "<summary>Import: chess.com only</summary>",
    "",
    "#### Win chance",
    "",
    "A token `{{num:<path>}}` and a <placeholder>.",
    "",
    "</details>",
    "<!-- how-it-works:end -->",
    "## Quick start",
  ].join("\n");

  it("takes only the marked section", () => {
    const out = howItWorksPage(readme, "<figure>svg</figure>");
    expect(out).not.toContain("# Rematch");
    expect(out).not.toContain("Quick start");
    expect(out).not.toContain("how-it-works:start");
  });
  it("puts the diagram in place of the mermaid block", () => {
    const out = howItWorksPage(readme, "<figure>svg</figure>");
    expect(out).toContain("<figure>svg</figure>");
    expect(out).not.toContain("mermaid");
  });
  it("sends decisions to their pages and source files to GitHub", () => {
    const out = howItWorksPage(readme, "");
    expect(out).toContain("](/rematch/docs/decisions/0090-missed-moments-trainer/)");
    expect(out).toContain(
      "](https://github.com/Terkea/rematch/blob/main/backend/crates/chess-core/src/score.rs)",
    );
  });
  it("lifts headings one level and keeps the disclosure tags", () => {
    const out = howItWorksPage(readme, "");
    expect(out).toContain("\n## One blunder\n");
    expect(out).toContain("\n### Win chance\n");
    expect(out).toContain("<details>\n<summary>Import: chess.com only</summary>");
    expect(out).toContain("</details>");
    expect(out).toContain("`{{num:<path>}}` and a &lt;placeholder&gt;.");
  });
  it("fails loudly when the markers are missing", () => {
    expect(() => howItWorksPage("# Rematch", "")).toThrow(/how-it-works/);
  });
});

describe("helpers", () => {
  it("reads the title and the slug", () => {
    expect(titleOf("# 0090. The trainer\n\nBody")).toBe("0090. The trainer");
    expect(slugOf("ADR-R2-12-local-syzygy-probe.md")).toBe("adr-r2-12-local-syzygy-probe");
  });
  it("turns the Tailwind theme block into plain custom properties", () => {
    const css = tokensToPlainCss(
      '@custom-variant dark (&:where([data-theme="dark"]));\n@theme static {\n  --color-*: initial;\n  --color-ink: x;\n}\n',
    );
    expect(css).toContain(":root {\n  --color-ink: x;\n}");
    expect(css).not.toContain("@theme");
    expect(css).not.toContain("initial");
    expect(css).not.toContain("@custom-variant");
  });
});
