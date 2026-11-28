// GitHub's maths syntax on the site (ADR 0118): $...$ inline, $$...$$ and ```math fences for
// display, rendered at build time by KaTeX. astro.config.mjs and test/math.test.ts share this, so
// the test renders exactly what the site does. KaTeX's CSS and fonts are bundled from node_modules
// (astro.config.mjs customCss), never fetched from a CDN (ADR 0096).
import rehypeKatex from "rehype-katex";
import remarkMath from "remark-math";
import remarkMathFence from "./remark-math-fence.mjs";

/** @type {import("rehype-katex").Options} */
export const KATEX_OPTIONS = {
  // A formula KaTeX cannot parse shows its source in a token colour (pen red means a mistake).
  errorColor: "var(--color-ink-soft)",
  // Unicode in TeX, an unknown command: warn at build time rather than render something else.
  strict: "warn",
};

export const remarkPlugins = [remarkMath, remarkMathFence];
/** @type {any[]} */
export const rehypePlugins = [[rehypeKatex, KATEX_OPTIONS]];
