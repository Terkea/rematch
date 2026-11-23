// The Rematch site: a landing page (src/pages/index.astro) and Starlight docs under /docs/.
// Served from GitHub Pages at https://terkea.github.io/rematch/ (ADR 0096).
import starlight from "@astrojs/starlight";
import { defineConfig, passthroughImageService } from "astro/config";

const REPO = "https://github.com/Terkea/rematch";

export default defineConfig({
  site: "https://terkea.github.io",
  base: "/rematch",
  trailingSlash: "always",
  // Screenshots are plain files in public/, so no image pipeline (and no sharp) is needed.
  image: { service: passthroughImageService() },
  // Code is set in the app's own type on a well (DESIGN.md: no monospace, no rainbow tokens).
  markdown: { syntaxHighlight: false },
  integrations: [
    starlight({
      title: "Rematch",
      description:
        "Rematch, chess improver: an open-source coach that reads your chess.com games, names the mistakes you repeat and drills them. Stop losing to the same move.",
      favicon: "/favicon.svg",
      expressiveCode: false,
      pagefind: true,
      credits: false,
      social: [{ icon: "github", label: "Rematch on GitHub", href: REPO }],
      editLink: { baseUrl: `${REPO}/edit/main/site/` },
      customCss: [
        "./src/styles/fonts.css",
        "./src/styles/generated/tokens.css",
        "./src/styles/docs.css",
      ],
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Getting started", link: "/docs/getting-started/" },
            { label: "What Rematch does", link: "/docs/features/" },
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
