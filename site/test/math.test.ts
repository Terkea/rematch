import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createMarkdownProcessor } from "@astrojs/markdown-remark";
import { describe, expect, it } from "vitest";
import { rewriteMarkdown } from "../scripts/content-lib.mjs";
import { loadExperiments, mathPage } from "../scripts/lab-lib.mjs";
import { rehypePlugins, remarkPlugins } from "../scripts/math-config.mjs";
import remarkMathFence from "../scripts/remark-math-fence.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(HERE, "fixtures", "lab");

/** Renders markdown as the site does: Astro's remark pipeline with the plugins astro.config passes
 * to `unified()`. */
async function render(markdown: string): Promise<string> {
  const processor = await createMarkdownProcessor({
    syntaxHighlight: false,
    remarkPlugins,
    rehypePlugins,
  } as never);
  return (await processor.render(markdown)).code;
}

describe("remarkMathFence", () => {
  it("turns a ```math code node into a display math node and leaves other code alone", () => {
    const tree = {
      type: "root",
      children: [
        { type: "code", lang: "math", meta: null, value: "a^2 + b^2" },
        { type: "code", lang: "sh", meta: null, value: "ls" },
        { type: "blockquote", children: [{ type: "code", lang: "math", value: "x" }] },
      ],
    };
    (remarkMathFence() as (t: unknown) => void)(tree);
    const [math, sh, quote] = tree.children as Array<Record<string, any>>;
    expect(math?.type).toBe("math");
    expect(math?.value).toBe("a^2 + b^2");
    expect(math?.data.hChildren[0].properties.className).toEqual(["language-math", "math-display"]);
    expect(sh?.type).toBe("code");
    expect(quote?.children[0].type).toBe("math");
  });
});

describe("maths rendering", () => {
  it("renders inline, $$ and ```math maths with KaTeX", async () => {
    const html = await render(
      "Inline $a^2$.\n\n$$\nb^2\n$$\n\n```math\nc^2\n```\n\n```sh\nls\n```\n",
    );
    expect(html).toContain('class="katex"');
    expect(html.match(/class="katex-display"/g)?.length).toBe(2);
    expect(html).not.toContain("$$");
    expect(html).not.toContain("language-math");
    expect(html).toContain("<pre"); // the shell fence stays code
    expect(html).not.toContain("katex-error");
  });

  it("renders a fixture experiment's maths page with no raw TeX left", async () => {
    const exp = loadExperiments(FIXTURE).find((e) => e.folder === "EXP-9001");
    const text = readFileSync(join(FIXTURE, "experiments", "EXP-9001", "math.md"), "utf8");
    const html = await render(mathPage(exp as never, text));
    expect(html).toContain('class="katex"');
    expect(html).toContain('class="katex-display"');
    expect(html).not.toContain("$$");
    expect(html).not.toMatch(/\$\\hat/);
    expect(html).not.toContain("katex-error");
    expect(html).not.toContain("&#x3C;!--");
    // The placeholder is escaped prose, the maths' < and \langle are not.
    expect(html).toContain("&#x3C;word>");
    expect(html).toContain("⟨");
  });

  it("keeps prices in a rewritten page as text", async () => {
    const html = await render(rewriteMarkdown("Gold is $60/year and Diamond $144/year.", "x.md"));
    expect(html).not.toContain("katex");
    expect(html).toContain("$60/year and Diamond $144/year");
  });
});
