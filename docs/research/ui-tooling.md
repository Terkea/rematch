# UI Tooling Research: Getting Premium, Non-Generic UI out of Claude Code

Researched: 2026-11-21. Target stack: Next.js (App Router) + TypeScript + Tailwind, built with Claude Code on native Windows 11.

Package versions and licenses below were read directly from the npm registry (`npm view <pkg> version license`) on the research date. Everything else is sourced inline. Where sources disagree, that is noted.

---

## 0. TL;DR

The tool you pick matters less than three habits that keep coming up across sources:

1. **Fix the decisions before Claude writes code.** Write down tokens, type, palette, radius, motion rules and a ban list. A design-system skill plus CLAUDE.md does this.
2. **Give Claude references instead of adjectives.** Two or three annotated screenshots of real high-quality products work far better than "make it modern and premium".
3. **Close the visual loop.** Claude renders the page, screenshots it, critiques it against the references and the ban list, and fixes it. Use Playwright (MCP or CLI) for this, with Chrome DevTools MCP for performance and debugging.

Component kits (shadcn, Aceternity, Magic UI) don't make a UI premium. Used at their defaults, they are what makes it look generic.

---

## 1. Why AI UIs look the same (the problem we are designing against)

- Adrian Krebs scored about 1,500 Show HN submissions (April 2026) for AI design tells. 22% had 4+ tells and 32% had 2–3. The tells were: Inter everywhere, "VibeCode Purple", permanent dark mode with grey body text, a badge above the hero H1, gradients and big glows, colored top/left borders on cards, identical icon-topped feature cards, numbered 1-2-3 steps, stat banner rows, all-caps labels, glassmorphism, and stock shadcn/ui. ([adriankrebs.ch](https://www.adriankrebs.ch/blog/design-slop/))
- The usual explanation is that these are the averaged defaults of Tailwind UI (`bg-indigo-500`) and years of scraped Tailwind tutorials. ([prg.sh](https://prg.sh/ramblings/Why-Your-AI-Keeps-Building-the-Same-Purple-Gradient-Website), [youware](https://www.youware.com/blog/how-we-escaped-the-purple-prison-of-ai-frontends))
- HN commenters add more tells: monospaced numbers, italic serif headings, blue/purple/green palettes with glows. ([Northeast Times summary](https://northeasttimes.com/2026/07/31/ai-design-tools-are-making-every-website-look-the-same/))
- Anthropic's current `frontend-design` skill names **five cliché looks** that Claude itself tends toward. Note these, because the "fix" for slop is often a second kind of slop:
  1. Warm cream (#F4F1EA) + high-contrast serif + terracotta accent (#D97757)
  2. Near-black + one acid-green or vermilion accent
  3. Broadsheet layout with hairline rules, zero radius, newspaper columns
  4. SaaS-card kit: identical rounded cards, one radius, the same soft grey shadow, gradient washes
  5. Template chrome: tracked ALL-CAPS eyebrows, "A · B · C" meta strings, "WORD — fragment" labels, #0B0B0B/#111 near-black, monospace data labels, "→" on every link
  ([anthropics/skills frontend-design SKILL.md](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md))

**Implication for a chess product:** look and feel should come from the subject matter (boards, notation, clocks, the tournament hall, annotated books like Informant/Chess Life), not from SaaS startup templates.

---

## 2. Claude Code skills and plugins for UI/design

### 2.1 Official and first-party

| Skill / plugin | What it does | Install | Notes |
|---|---|---|---|
| **Anthropic `frontend-design`** | Before any CSS, it makes Claude write a token plan (4–6 named hex colors, typefaces and roles, ASCII wireframes, principles). It then reviews the plan for generic defaults, builds, and critiques with screenshots. It names the 5 cliché looks above. Motion rule: "one orchestrated moment beats scattered effects"; fade-up on every section is called generic. It also covers copywriting rules (sentence case, verbs like "Save changes"). | `/plugin install frontend-design@claude-plugins-official` (already available in this environment as `frontend-design:frontend-design`) | Reportedly rewritten in June 2026 (blacklist replaced by look descriptions plus plan-and-critique) and revised Sept 2026. Several reviewers say it is **tuned for expressive marketing pages more than dense product UI**. ([SKILL.md](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md), [Anthropic blog](https://claude.com/blog/improving-frontend-design-through-skills), [Subframe review](https://www.subframe.com/tips/best-claude-code-skills-for-ui-design)) |
| **Anthropic "Prompting for frontend aesthetics" cookbook** | A ~400-token aesthetics block (typography, color/theme, motion, backgrounds) meant for a system prompt or CLAUDE.md. | Copy text | ([cookbook](https://platform.claude.com/cookbook/coding-prompting-for-frontend-aesthetics)) |
| **Claude Design** (Anthropic Labs) | A canvas prototyping product, launched 2026-04-17. In June 2026 it gained design-system import, two-way sync with Claude Code, and a `/design` command. | claude.ai paid plans | Useful for exploring directions before coding. Token cost is the main complaint. ([VentureBeat](https://venturebeat.com/technology/anthropic-ships-major-claude-design-overhaul-with-design-system-imports-code-round-trips-and-a-fix-for-its-token-burning-problem), [TheNewStack](https://thenewstack.io/anthropic-claude-design-overhaul/), [Anthropic](https://claude.com/blog/claude-design-stays-on-brand-for-daily-work)) |
| **Vercel `web-design-guidelines`** | Audits UI code against the Vercel Web Interface Guidelines (100+ rules for a11y, focus, forms, motion, performance). Rules are fetched at runtime. | `npx skills add vercel-labs/agent-skills --skill web-design-guidelines` | A quality floor, not taste. ([listing](https://mcpservers.org/agent-skills/vercel/web-design-guidelines), [rules](https://github.com/vercel-labs/web-interface-guidelines)) |
| **Vercel `vercel-react-best-practices`** | React/Next performance rules (waterfalls, bundle size, re-renders). | `npx skills add vercel-labs/agent-skills --skill vercel-react-best-practices` | ([skills.sh](https://skills.sh/topic/react)) |
| **Vercel `react-view-transitions`** | View Transitions guidance for React/Next. | same repo | ([nextjs reference](https://raw.githubusercontent.com/vercel-labs/agent-skills/main/skills/react-view-transitions/references/nextjs.md)) |
| **GSAP official skills** | Core, timelines, ScrollTrigger, plugins, `useGSAP` and cleanup in React, performance. | `/plugin marketplace add greensock/gsap-skills` | Install from `greensock/`, since third-party repackages exist. ([repo](https://github.com/greensock/gsap-skills)) |
| **Motion AI Kit** | `/motion` skill + Motion MCP server (docs search, best practices). | `npx motion-ai` | Best practices and docs are free. Examples search, "Performance Doctor" and the spring generator need paid Motion+. ([motion.dev/ai-kit](https://motion.dev/ai-kit)) |
| **shadcn MCP** (official) | Browse, search and install components from shadcn and custom registries. | `npx shadcn@latest mcp init --client claude` | If it shows "No tools", run `npx clear-npx-cache`. ([ui.shadcn.com/docs/mcp](https://ui.shadcn.com/docs/mcp)) |
| **dataviz** skill | Chart form, palette and a validator, interaction rules. | Already available in this environment | Relevant for eval graphs and accuracy charts. |

### 2.2 Community design skills (most cited)

| Skill | Approach | Install | Verdict |
|---|---|---|---|
| **Impeccable** (Paul Bakaus, Apache-2.0) | Builds on frontend-design. About 20+ commands (`/critique`, `/audit`, `/polish`, ...), an anti-pattern catalog (25–27 rules depending on source), and a **deterministic CLI detector** `npx impeccable detect src/` that needs no LLM. | `/plugin marketplace add pbakaus/impeccable` | The most useful for **critique passes**, because a deterministic linter catches slop the model doesn't notice. ([README](https://cdn.jsdelivr.net/npm/impeccable@2.3.2/README.repo.md), [mejba review](https://www.mejba.me/blog/impeccable-claude-code-design-skill)) |
| **taste-skill** (Leonxlnx, MIT) | A set of "anti-slop" skills: `design-taste-frontend` (v2 experimental, v1 kept), `high-end-visual-design`, `minimalist-ui`. | `/plugin marketplace add Leonxlnx/taste-skill` | Opinionated presets. Good to raid for rules, but risky as a whole aesthetic because it pushes its own house style. ([README](https://raw.githubusercontent.com/Leonxlnx/taste-skill/HEAD/README.md)) |
| **interface-design** (Dammyjay93) | Settles a design direction once and saves the decisions to `.interface-design/system.md`, so later sessions stay consistent. | `/plugin marketplace add Dammyjay93/interface-design` | Aimed at **product/dashboard UI**, which suits report pages and drills. Same idea as the project skill recommended below. ([pasqualepillitteri guide](https://pasqualepillitteri.it/en/news/576/claude-code-skills-design-uiux-guide)) |

Catalog roundups: [firecrawl](https://www.firecrawl.dev/blog/best-claude-code-skills), [Subframe](https://www.subframe.com/tips/best-claude-code-skills-for-ui-design), [Medium "9 best UI skills"](https://medium.com/@hii_mohit/9-best-ui-skills-for-claude-code-in-2026-i-tested-them-all-58fe42809376).

**Caution:** stacking several taste skills gives conflicting instructions, and the result regresses to the mean. Pick **one** generative design skill (frontend-design) and **one** critique tool (Impeccable). Everything else should be project-specific.

### 2.3 How people structure a project-level design-system skill

The pattern people converge on is to separate **evidence**, **rules** and **tokens** ([wavect.io](https://wavect.io/blog/claude-code-design-system-files/), [mowgli.ai](https://mowgli.ai/blog/claude-code-ui-design-workflow), [superdesign](https://superdesign.dev/blog/claude-code-ui-design)):

```
.claude/
  skills/
    design-system/
      SKILL.md            # when to load; non-negotiables; workflow; definition of done
      tokens.md           # human-readable token table (mirrors app/globals.css @theme)
      components.md       # which primitive to use for what; variants; states
      motion.md           # durations, easings, springs, what animates and what never does
      ban-list.md         # named anti-patterns (project-specific + the 5 cliché looks)
      references/         # 2-6 annotated screenshots + REFERENCES.md explaining WHAT to take from each
    chess-ui/
      SKILL.md            # board theme, piece set, arrows/highlights, eval-bar and eval-graph conventions
docs/design/DESIGN.md     # the one-page design direction (subject, audience, mood, signature element)
CLAUDE.md                 # short: points to the skill, states the loop, under ~200 lines
```

Key practices:
- **Tokens live in code, and the skill points at them.** Tailwind v4 `@theme` in `app/globals.css` is the source of truth. `tokens.md` explains intent ("`--color-accent` is used only for the user's move and primary CTA"). "Current tokens beat screenshots for exact values." ([wavect](https://wavect.io/blog/claude-code-design-system-files/))
- **The SKILL.md frontmatter `description` must trigger on UI work**, e.g. "Use whenever creating or changing any UI, page, component, style, animation, or chart in this repo."
- **Definition of done is visual.** Screenshots at 390px / 1280px in light and dark, with a ban-list check, before claiming done.
- **Log what has been tried** so later passes don't repeat it (frontend-design recommends this too).

### 2.4 CLAUDE.md UI rules (pattern)

Keep it short and imperative, and link out to the skill. Typical contents ([mowgli](https://mowgli.ai/blog/claude-code-ui-design-workflow), [superdesign](https://superdesign.dev/blog/how-to-make-claude-code-ui-look-good)):
- "Before any UI change, load `.claude/skills/design-system`."
- "Use only tokens from `@theme`. No raw hex, no arbitrary Tailwind values (`p-[13px]`) except in `components/chess/**`."
- "Never introduce a new font, color, radius, shadow or easing; propose it instead."
- Required states for every view: loading (skeleton matching layout), empty, error, and long-content.
- A11y floor: visible focus rings, `prefers-reduced-motion`, AA contrast, keyboard-operable board.
- "After a UI change: run the visual loop (section 6) and attach screenshots in your summary."

---

## 3. Visual feedback loops (and Windows reliability)

| Tool | Best for | Windows notes |
|---|---|---|
| **Playwright MCP** `@playwright/mcp` 0.0.86 (Apache-2.0) | Driving pages, multi-viewport screenshots, accessibility snapshots, interaction (clicking squares, dragging pieces). The default choice for the design loop. | Works on native Windows. Known pitfalls: (a) `npx` is `npx.cmd`, so stdio servers need a `cmd /c` wrapper; (b) `claude mcp add` has mangled `/c` into a path in some versions, so **edit `.mcp.json` by hand**; (c) "Browser is already in use" when your Chrome profile is open, so use `--isolated`; (d) run `npx playwright install chromium` if browsers are missing. ([claude-code#24144](https://github.com/anthropics/claude-code/issues/24144), [claudeissues 46360](https://claudeissues.com/issue/46360-windows-claude-mcp-add-breaks-c-flag-playwright-mcp-plugin-fails-working-workaro), [dev.to Windows setup](https://dev.to/ratamaha/claude-code-mcp-server-setup-on-windows-4clo), [playwright.dev MCP](https://playwright.dev/docs/getting-started-mcp)) |
| **Playwright CLI** (`playwright-cli`, Microsoft) + skill | Same capabilities, with state kept on disk instead of in context. Microsoft now recommends it for coding agents with shell access. Headless by default. | No MCP spawn issues on Windows, since it runs as a plain shell command. Token savings are disputed: Microsoft claims about 4x, while [Checkly](https://www.checklyhq.com/blog/mcp-vs-cli-token-efficiency/) measured roughly parity. ([README](https://raw.githubusercontent.com/microsoft/playwright-cli/main/README.md), [playwright.dev](https://playwright.dev/agent-cli/introduction)) |
| **Plain Playwright test script** `scripts/shots.ts` | Deterministic screenshots of a fixed route list × viewports × themes, saved to `.shots/`. Claude reads the PNGs. Also allows `toHaveScreenshot` visual regression later. | The most reliable option on Windows. No MCP at all. |
| **Chrome DevTools MCP** `chrome-devtools-mcp` 1.13.0 (Apache-2.0) | Console errors, network, **performance traces / LCP / CLS / INP**, Lighthouse-style checks, emulation. "Driving vs debugging": use it for measuring, not for the main design loop. | Node 20.19+. **Run from PowerShell/Git Bash, not WSL** (launching Windows Chrome from WSL fails). The Windows "MCP error -32000" discovery issue is covered in the troubleshooting doc; the fixes are a `cmd /c` wrapper and a longer startup timeout. Its tool description itself says to prefer snapshots over screenshots. ([Chrome docs](https://developer.chrome.com/docs/devtools/agents/get-started), [troubleshooting](https://raw.githubusercontent.com/ChromeDevTools/chrome-devtools-mcp/HEAD/docs/troubleshooting.md), [Steve Kinney](https://stevekinney.com/writing/driving-vs-debugging-the-browser)) |
| **Claude in Chrome / built-in browser pane** | Interactive viewing in your real browser session. | Fine for ad-hoc checks. Less scriptable than Playwright. ([comparison](https://mintlify.com/shanraisshan/claude-code-best-practice/reports/browser-automation-mcp)) |

Comparisons: [dev.to "tested every browser tool"](https://dev.to/minatoplanb/i-tested-every-browser-automation-tool-for-claude-code-heres-my-final-verdict-3hb7), [heyuan110 screenshot MCP setup](https://www.heyuan110.com/posts/ai/2026-07-21-claude-code-screenshot-mcp-frontend-debugging/), [Argos on Playwright MCP + visual testing](https://argos-ci.com/blog/playwright-mcp-visual-testing).

**Critique realism:** "the same agent that wrote the code cannot be trusted to judge its own output" ([mowgli](https://mowgli.ai/blog/claude-code-ui-design-workflow)). Run the critique in a **fresh subagent** given only the screenshots, the references and the ban list, plus the deterministic `impeccable detect`.

---

## 4. Component and animation libraries

Versions are from the npm registry on 2026-11-21.

### 4.1 Core stack

| Package | Version | License | Role / notes |
|---|---|---|---|
| `next` | 16.4.6 | MIT | App Router. `experimental.viewTransition` is **still experimental** in the docs. `transitionTypes` shipped in 16.2 without the flag. ([docs](https://nextjs.org/docs/app/api-reference/config/next-config-js/viewTransition), [16.2](https://nextjs.org/blog/next-16-2)) |
| `react` | 19.3.1 | MIT | `<ViewTransition>` (canary/experimental export; check before relying on it) |
| `tailwindcss` | 4.3.5 | MIT | CSS-first `@theme` tokens, OKLCH colors |
| `shadcn` (CLI) | 4.23.0 | MIT | Copies source into your repo. Choose base and style at init (see 4.2). |
| `radix-ui` | 1.8.1 | MIT | Unified Radix primitives package |
| `@base-ui/react` | 1.9.0 | MIT | Base UI (MUI team). **New shadcn projects default to Base UI since July 2026.** ([changelog](https://ui.shadcn.com/docs/changelog/2026-07-base-ui-default)) |
| `motion` | 14.14.0 | MIT | Formerly framer-motion. Import from `motion/react`. Layout animations, `AnimatePresence`, springs, gestures, `useScroll`. Motion+ is a separate paid tier. ([repo](https://github.com/motiondivision/motion)) |
| `gsap` / `@gsap/react` | 3.15.1 / 2.1.2 | GSAP Standard "no charge" license (not OSI) | **Free for commercial use including all former Club plugins** (SplitText, MorphSVG, ScrollSmoother, DrawSVG...) since Webflow's acquisition (v3.13, Apr 2025). The license restricts building **competing no-code visual animation tools**, which doesn't affect us. ([license](https://gsap.com/community/standard-license/), [Webflow](https://webflow.com/blog/gsap-becomes-free), [3.13](https://gsap.com/blog/3-13/)) |
| `lenis` | 1.3.27 | MIT | Smooth scroll. Use on the landing page only, never in app pages (it hurts native feel and accessibility). Pair it with GSAP ScrollTrigger if you need scroll-driven animation. |
| `tw-animate-css` | 1.4.0 | MIT | shadcn's enter/exit utilities (replaces tailwindcss-animate) |
| `@number-flow/react` | 0.6.2 | MIT | Animated number transitions. Good for eval numbers, rating deltas, accuracy %. |
| `sonner` 2.0.8, `vaul` 1.1.2, `cmdk` 1.1.1 | | MIT | Toasts, drawers (mobile drill sheets), command palette |
| `next-themes` | 0.4.6 | MIT | Theme switching |
| `three` / `@react-three/fiber` / `@react-three/drei` | 0.187.0 / 9.8.2 / 10.7.9 | MIT | **Probably skip.** A 3D board or hero is an obvious slop/gimmick risk and costs a lot of bundle. Use it only for one deliberate landing moment, lazy-loaded. |
| Icons: `lucide-react` 1.77.0 (ISC), `@phosphor-icons/react` 2.1.10 (MIT) | | | Lucide is the shadcn default and so part of the stock look. Phosphor (with weights) or custom chess glyphs help differentiate. |
| Fonts: `next/font` (Google or local) | | OFL mostly | Avoid Inter / Geist / Space Grotesk / Instrument Serif as the default pick (Krebs tells). Choose deliberately and document why. |

### 4.2 shadcn/ui without the shadcn look

- Since Dec 2025, `npx shadcn create` offers **styles** (Vega = classic look, Nova = compact, Maia = soft, Lyra = sharp/boxy, Mira = dense; later Luma, Rhea, Sera) on a **base** (Radix, Base UI, React Aria). Configured as `{base}-{style}` in `components.json`. ([shadcn create](https://ui.shadcn.com/docs/changelog/2025-12-shadcn-create), [shadcnblocks overview](https://www.shadcnblocks.com/blog/shadcn-component-styles-vega-nova-maia-lyra-mira), [React Aria](https://ui.shadcn.com/docs/changelog/2026-07-react-aria))
- **Don't use Vega with default zinc/neutral and `--radius: 0.625rem`.** That combination is the stock look. For data-dense report and drill pages, **Mira or Nova** are a better base. Then:
  - Rewrite semantic tokens (`--background`, `--card`, `--primary`, `--ring`, ...) in OKLCH from your own palette. Theme editors like tweakcn help. ([OKLCH theming guide](https://dev.to/yigit-konur/the-complete-shadcnui-theming-guide-a-practical-approach-with-oklch-to-make-it-looks-10x-more-2l4l))
  - Use **several radii by role** (board 2px, cards 10px, pills full) instead of one radius everywhere, which is cliché #4.
  - Replace the default card + soft-shadow with surfaces made of tonal layering, borders, or inset wells.
  - Restyle focus rings, selection and scrollbars, and pick a non-Lucide icon set.
  - Treat shadcn as **behavior + a11y scaffolding**. The visual identity lives in tokens and your own composite components.

### 4.3 Aceternity UI / Magic UI: slop risk

- **Magic UI**: free components MIT; Pro templates paid. ([designrevision](https://designrevision.com/alternatives/magic-ui))
- **Aceternity UI**: the official license is a **custom "Aceternity License"**, not MIT. End products are allowed, redistribution and resale are not. Pro costs about $199 one-time. ([licence](https://ui.aceternity.com/licence))
- **Slop risk: high.** Aceternity's 3D cards, beams, spotlight and magnetic buttons, and Magic UI's Rainbow/Shimmer buttons, marquee and animated beams, are "on every second dev-tool landing page" ([21st.dev](https://21st.dev/blog/magic-ui-alternatives), [jeangalea](https://jeangalea.com/ai-ui-looks-generic/)). Claude has seen these thousands of times and reaches for them when asked for "premium".
- **Rule:** never import these wholesale. If one effect is truly the right "signature moment", re-implement it from scratch with Motion/GSAP in your own token language. Put the library names in the ban list.

### 4.4 Chess-specific libraries (related licensing note)

| Package | Version | License | Note |
|---|---|---|---|
| `react-chessboard` | 5.12.1 | MIT | React-native API; fully themeable squares, pieces and arrows |
| `chessground` (Lichess) | 9.2.1 | **GPL-3.0-or-later** | Excellent UX, but **copyleft**: it would force GPL on the app if distributed. Avoid unless the project is open source. |
| `chess.js` | 1.4.0 | BSD-2-Clause | Move legality, PGN/FEN |
| `recharts` 3.11.0 / `@visx/visx` 4.0.0 | | MIT | Eval graph. visx (low-level, D3-based) gives more distinctive results than stock Recharts. |

The board is the product's "signature element". Invest in a custom board theme (square colors from the palette, a custom or licensed piece set, last-move and threat highlights, arrow styles, move-quality glyphs) rather than in decorative landing effects.

---

## 5. Workflows: what actually produces premium results

### 5.1 What works (consistently reported)

1. **Reference-driven design.** Collect 2–3 references per surface. Save the screenshots in the repo and **annotate what to take from each** ("take the type scale and density from Linear's issue view; take the chart restraint from Stripe's dashboard; do NOT copy layout"). Claude copies structure it can see far better than it follows adjectives. ([superdesign](https://superdesign.dev/blog/claude-code-ui-design), [UX Planet](https://uxplanet.org/6-tricks-to-level-up-claude-design-7ed86375be49), [aidesigner](https://www.aidesigner.ai/blog/claude-code-frontend-design))
   - Sources: Mobbin (real product flows), Godly and Land-book (landing pages), Refero, Dribbble only for mood. Good targets for this product: Linear, Stripe dashboard, Vercel dashboard, Raycast, Arc, and in the chess domain Lichess analysis, Chessable, Aimchess, plus print chess books and magazines for editorial report pages.
2. **Tokens first, pages second.** Have Claude write the plan (palette, type scale, spacing, radii, elevation, motion) and **review the plan for clichés before coding**. This is the core of frontend-design. Then build a single `/styleguide` route that renders every token and primitive, and iterate there until it's right. ([frontend-design](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md), [wavect](https://wavect.io/blog/claude-code-design-system-files/))
3. **Generate several directions, then choose.** Ask for 3 distinct directions (different type, color and layout philosophy) as static mockups on `/lab/*` routes, screenshot them, and pick one. Don't polish the first decent output. ([aifire](https://www.aifire.co/p/15-claude-ai-design-tricks-for-stunning-websites-in-2026))
4. **Real content from day one.** Use actual game data, real PGNs, real move annotations. Lorem ipsum hides density problems, and good report pages depend on handling density well.
5. **Screenshot → critique → fix loop**, with a fresh critic agent and a deterministic linter (section 6).
6. **Spend boldness in one place.** One memorable element per surface: the board on analysis pages, the eval graph on reports, one orchestrated hero moment on the landing page. Everything else stays quiet. ([frontend-design](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md))

### 5.2 Figma, v0, Claude Design

- **Figma → code** (Figma MCP + `figma:figma-design-to-code`, Code Connect) is excellent **if a designer produces real Figma**. Without a designer it is an extra hop. The Figma plugin is available in this environment.
- **Claude Design** works well for quick visual exploration of directions, which it can then hand off to Claude Code. It costs a lot of tokens.
- **v0** outputs shadcn-default UI. It's useful for scaffolding behavior but makes the "stock" problem worse. Not recommended as a design source.
- **Superdesign / AIDesigner / Subframe** are vendor tools that promote their own workflow. Their blog advice (references, tokens, ban lists) is sound and can be done without them.

### 5.3 What produces generic output

- Adjective prompts: "modern, sleek, premium, beautiful, clean".
- Asking for "a landing page" without a subject-specific concept.
- Installing Aceternity/Magic UI blocks or v0 output and "tweaking".
- Stock shadcn tokens, a single radius, Lucide icons, Inter/Geist.
- Letting Claude choose fonts and colors on each page (drift), or no persisted design memory across sessions.
- Animating everything: fade-up on every section, hover-lift on every card, parallax for its own sake.
- Having the same agent judge its own work without screenshots.

---

## 6. Do / Don't list for instructing Claude

**Do**
- Do state the **subject, audience and primary job** of each surface ("a 1500-rated club player reviewing yesterday's loss on a laptop").
- Do give **2–3 annotated references** and say what to borrow and what not to copy.
- Do lock **exact values**: font families and weights, the type scale, OKLCH palette with named roles, spacing scale, radii by role, shadow and elevation rules, durations and easings.
- Do provide a **named ban list** (section 1 tells + the 5 cliché looks + Aceternity/Magic UI effects + "→" on every link + gradient text + glassmorphism + emoji icons).
- Do require a **plan → self-review → build → screenshot critique** sequence, and ask Claude to state what it changed after critique.
- Do specify **motion with intent**: what animates (state changes the user caused: move played, drill solved, panel opened), how (spring stiffness/damping or duration/easing tokens), and what never animates. Respect `prefers-reduced-motion`.
- Do demand **all states**: loading skeleton, empty, error, long content, mobile.
- Do use **real data** and write real copy (sentence case, verbs like "Review game", "Start drill").
- Do ask for **3 divergent directions** at the start of a new surface.
- Do keep design memory in files (skill + DESIGN.md + tokens), not in chat history.

**Don't**
- Don't say "make it look premium/modern/clean". Describe concrete properties instead.
- Don't let Claude introduce new tokens, fonts or colors ad hoc. Have it propose them, then you approve.
- Don't default to dark mode with grey text, purple/indigo accents, gradient blobs, glows, glass cards, or a badge above the hero H1.
- Don't use icon-topped 3-up feature grids, stat-banner rows or "1-2-3 steps" unless the content really has that shape.
- Don't use ALL-CAPS tracked eyebrows, middle-dot meta strings, or accent one italic-serif word in a headline.
- Don't use one radius and one shadow everywhere.
- Don't use Lenis, parallax or scroll-jacking in the app (only on the landing page, if at all).
- Don't paste component-library effects. Re-implement the single signature moment yourself.
- Don't stack multiple taste skills. One generator, one critic.
- Don't accept "done" without screenshots at mobile and desktop, in both themes.

---

## 7. Recommended setup for this project

### 7.1 Packages (install later; nothing was installed during research)

```bash
# scaffold
npx create-next-app@latest  # Next 16.4.x, TS, Tailwind v4, App Router, ESLint
npx shadcn@latest init      # choose base: Base UI (default) or Radix; style: Mira or Nova (NOT Vega); then rewrite tokens

# runtime
npm i motion gsap @gsap/react lenis @number-flow/react sonner vaul cmdk next-themes \
      @phosphor-icons/react chess.js react-chessboard @visx/visx tw-animate-css \
      class-variance-authority clsx tailwind-merge

# dev
npm i -D @playwright/test && npx playwright install chromium
```

Pinned reference versions (2026-11-21): next 16.4.6, react 19.3.1, tailwindcss 4.3.5, shadcn 4.23.0, @base-ui/react 1.9.0 / radix-ui 1.8.1, motion 14.14.0, gsap 3.15.1, @gsap/react 2.1.2, lenis 1.3.27, react-chessboard 5.12.1, chess.js 1.4.0, @visx/visx 4.0.0, @number-flow/react 0.6.2.

Roles:
- **Motion** handles all UI and app animation (layout, presence, springs, board-adjacent micro-interactions).
- **GSAP** (+ ScrollTrigger/SplitText) only on the **landing page** for one orchestrated scroll story, with **Lenis** on the landing page only.
- **Avoid**: Aceternity UI, Magic UI, three/R3F (unless one deliberate, lazy-loaded hero), chessground (GPL).

### 7.2 MCP servers (`.mcp.json`, edited by hand on Windows)

```json
{
  "mcpServers": {
    "playwright": {
      "command": "cmd",
      "args": ["/c", "npx", "-y", "@playwright/mcp@latest", "--isolated", "--viewport-size=1280,800"]
    },
    "chrome-devtools": {
      "command": "cmd",
      "args": ["/c", "npx", "-y", "chrome-devtools-mcp@latest", "--isolated"]
    },
    "shadcn": {
      "command": "cmd",
      "args": ["/c", "npx", "-y", "shadcn@latest", "mcp"]
    }
  }
}
```
- Run Claude Code from PowerShell or Git Bash, **not WSL**.
- If a server shows "connected" but has no tools, run `npx clear-npx-cache` or point at the full path from `where npx`.
- Context7 (already configured) covers current Next/Motion/GSAP/Tailwind docs.
- Optional: the Motion AI Kit MCP (`npx motion-ai`) for Motion docs search.
- Fallback if the MCPs are flaky: the `scripts/shots.ts` Playwright script (below) needs no MCP.

### 7.3 Skills

- Global/plugin: **`frontend-design`** (Anthropic; already present), **Impeccable** (`/plugin marketplace add pbakaus/impeccable`, used for `/critique` and `npx impeccable detect`), **greensock/gsap-skills**, **vercel-labs `web-design-guidelines`** + **`vercel-react-best-practices`**, the **dataviz** skill (already present) for eval graphs.
- Skip taste-skill and interface-design as installed skills. Borrow their ideas into the project skill instead (interface-design's persisted `system.md` is the same as our project skill).
- Project: create `.claude/skills/design-system/` (structure in section 2.3) and `.claude/skills/chess-ui/` (board theme, piece set, highlight/arrow colors, move-quality glyph colors, eval bar and graph conventions, drill feedback animations).

### 7.4 CLAUDE.md UI section (short)

- Load `design-system` (and `chess-ui` for board/graph work) before any UI change.
- Tokens only (`@theme` in `app/globals.css`). No new fonts, colors, radii, shadows or easings without asking.
- Motion: Motion for UI, GSAP only in `app/(marketing)`. Durations and springs come from `motion.md`. Respect reduced motion.
- Every view ships with loading, empty, error and mobile states.
- Done = `npm run shots` passes and screenshots were reviewed against `ban-list.md` and the references. Report what the critique changed.

### 7.5 The workflow loop

1. **Direction (once):** write `docs/design/DESIGN.md` (subject, audience, mood, the signature element per surface). Collect 2–3 annotated references per surface into `.claude/skills/design-system/references/`. Optionally explore in Claude Design.
2. **Tokens (once):** Claude runs the frontend-design plan (palette, type, layout, principles) and self-reviews it against the 5 cliché looks and the ban list. You approve. It is encoded in `@theme` and the shadcn semantic tokens, and rendered on `/styleguide`.
3. **Directions per new surface:** 3 divergent static mockups on `/lab/<surface>-a|b|c` with real game data. Screenshot them and pick one.
4. **Build** with the project skills loaded.
5. **Screenshot:** `scripts/shots.ts` (Playwright) captures each changed route at 390×844 and 1440×900, light and dark, into `.shots/`. Or use Playwright MCP interactively, for example to play moves on the board and capture states.
6. **Critique:** a **fresh subagent** gets only the screenshots, the references, `ban-list.md` and DESIGN.md, and returns ranked issues. `npx impeccable detect src/` and the `web-design-guidelines` audit run alongside.
7. **Fix → repeat** steps 5–6 (usually 2–3 passes). Log the attempts in the skill so later passes don't repeat them.
8. **Measure (per milestone):** Chrome DevTools MCP performance trace on the landing, report and analysis pages (LCP/CLS/INP, animation jank, bundle size from GSAP/visx).
9. **Lock in:** add Playwright `toHaveScreenshot` baselines for key pages so later changes can't silently degrade the design.
