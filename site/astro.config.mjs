// The Rematch site: a landing page (src/pages/index.astro) and Starlight docs under /docs/.
// Served from GitHub Pages at https://terkea.github.io/rematch/ (ADR 0096).
import { existsSync } from "node:fs";
import { unified } from "@astrojs/markdown-remark";
import starlight from "@astrojs/starlight";
import { defineConfig, passthroughImageService } from "astro/config";
import { rehypePlugins, remarkPlugins } from "./scripts/math-config.mjs";

const REPO = "https://github.com/Terkea/rematch";
// The algorithm's maths page exists once algorithm/MATH.md does (scripts/sync-content.mjs).
const ALGORITHM_MATH = existsSync(new URL("../algorithm/MATH.md", import.meta.url));

export default defineConfig({
  site: "https://terkea.github.io",
  base: "/rematch",
  trailingSlash: "always",
  // Screenshots are plain files in public/, so no image pipeline (and no sharp) is needed.
  image: { service: passthroughImageService() },
  // Code is set in the app's own type on a well (DESIGN.md: no monospace, no rainbow tokens).
  // Maths in GitHub's syntax ($...$, $...$, ```math), rendered by KaTeX at build time (ADR 0118).
  markdown: { syntaxHighlight: false, processor: unified({ remarkPlugins, rehypePlugins }) },
  integrations: [
    starlight({
      title: "Rematch",
      description:
        "Rematch, chess improver. Chess, taught the way you learn: a patient coach that learns your patterns from your chess.com games, re-evaluates them after every game and points you at the one thing that will take you higher.",
      favicon: "/favicon.svg",
      // The mid-size mark (packages/ui/assets/brand), ink by day and cream at night, beside the
      // wordmark (docs.css sets it in Literata italic).
      logo: {
        light: "./src/assets/rematch-mark-day.svg",
        dark: "./src/assets/rematch-mark-night.svg",
        alt: "",
      },
      expressiveCode: false,
      pagefind: true,
      credits: false,
      social: [{ icon: "github", label: "Rematch on GitHub", href: REPO }],
      editLink: { baseUrl: `${REPO}/edit/main/site/` },
      customCss: [
        "./src/styles/fonts.css",
        "./src/styles/generated/tokens.css",
        "./src/styles/docs.css",
        "./src/styles/diagram.css",
        // KaTeX's stylesheet and fonts, bundled from node_modules, never a CDN (ADR 0096, 0118).
        "katex/dist/katex.min.css",
        "./src/styles/math.css",
      ],
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Getting started", link: "/docs/getting-started/" },
            { label: "What Rematch does", link: "/docs/features/" },
            { label: "How it works", link: "/docs/how-it-works/" },
            { label: "The honesty rules", link: "/docs/honesty/" },
          ],
        },
        {
          label: "Build it with us",
          items: [
            { label: "Architecture", link: "/docs/architecture/" },
            { label: "Contributing", link: "/docs/contributing/" },
            { label: "How we ship", link: "/docs/sdlc/" },
            { label: "Roadmap", link: "/docs/roadmap/" },
          ],
        },
        {
          label: "The lab",
          items: [
            { label: "Experiments", link: "/docs/experiments/" },
            { label: "Progress", link: "/docs/progress/" },
            ...(ALGORITHM_MATH
              ? [{ label: "The algorithm's maths", link: "/docs/algorithm/math/" }]
              : []),
          ],
        },
        {
          label: "Decision log",
          collapsed: true,
          items: [{ autogenerate: { directory: "docs/decisions" } }],
        },
        {
          label: "Research",
          collapsed: true,
          items: [{ autogenerate: { directory: "docs/research" } }],
        },
      ],
    }),
  ],
});
