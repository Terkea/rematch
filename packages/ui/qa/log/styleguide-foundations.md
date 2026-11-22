# Styleguide hero, foundations, primitives and overlays: critique log (M4 Task 9)

Scope: the first four sections of `/styleguide` (hero, foundations, primitives, overlays). The later sections belong to their own lanes.

Screenshots live outside the repo, in the task's scratchpad (`t9/`), and are not committed:

- `shots/styleguide/{390,1440}-{light,dark}.png`: full page from `scripts/shots.ts`.
- `r<n>/<section>-{390,1440}-{light,dark}.png`: one section per file (an element screenshot after hydration, reduced motion, theme pinned), rounds r1 to r6.
- `r4open/{dialog,drawer,menu,toast}-{390,1440}-{light,dark}.png`: each screen-taking overlay opened.
- `r5motion/1440-light-*.png` and `sheet.png`: the hero beat at 100 to 2700ms, motion on.

Every capture: no page errors, `scrollWidth` equal to the viewport at 390 (it was 407 before pass 1). At 1440 it is 1425, because the page scrollbar takes 15px.

`pnpm design:check`: 0 findings.

## How the states are shown

The hover and focus-visible cells come from the components' own CSS, not from copies of it. `forced-states.ts` copies every rule that uses `:hover` or `:focus-visible` from the page's stylesheets, swaps the pseudo-class for `[data-force~="hover"]` (or `"focus-visible"`), and drops the `(hover: hover)` media wrapper so a touch screenshot shows hover too. A styleguide cell then sets `data-force` on the element that owns the state. Unit tests cover the rewrite, Tailwind v4's nesting (`&:hover { @media (hover: hover) { … } }`), cascade layers, `:focus` against `:focus-visible`, and selector lists.

A state a primitive doesn't have is said in its cell ("Not offered: …", "No error look: …"), never drawn.

## Pass 1 (implementer, against `critique-prompt.md`; the lane forbids subagents)

```json
{
  "issues": [
    { "rank": 1, "severity": "major", "screenshot": "r1/primitives-390-dark.png", "observation": "Horizontal scroll at 390 (scrollWidth 407). Two state cells per line can't hold 'Delete account', the tabs or the segmented track; they spill out of the sheet.", "fix": "Below 640px one state per line: the state name in a 104px column, the specimen beside it." },
    { "rank": 2, "banId": "B4", "severity": "major", "screenshot": "r2/overlays-1440-dark.png", "observation": "Six equal cards, four of them holding one button each: the SaaS card grid.", "fix": "Keep the two overlays that can be pinned open as specimens; put the four screen-taking ones in one panel as a definition list with their openers." },
    { "rank": 3, "severity": "major", "screenshot": "r4/crop-prim-390-b.png", "observation": "A disabled switch that is on looks exactly like an enabled one: the ink track stays. DESIGN.md: disabled is faint, no fill.", "fix": "Switch: data-disabled + data-checked go to a well track with a rule edge; the thumb is rule-strong." },
    { "rank": 4, "severity": "major", "screenshot": "r3/hero-1440-light.png", "observation": "The hero graph's label lane names Nf4?? and Bxc3?? beside 28. Nd2??. DESIGN.md: the hero's lane names only the key move.", "fix": "Pass only the key move's judgement to the hero graph." },
    { "rank": 5, "severity": "minor", "screenshot": "r1/primitives-1440-light.png", "observation": "The button rows repeat the same 14-word sentence in every Error cell.", "fix": "Say it once in the family note; the cells show a dash (read as 'None')." },
    { "rank": 6, "severity": "minor", "screenshot": "r1/primitives-1440-light.png", "observation": "The segmented control's third option is cut off in its 1440 cell.", "fix": "Two options (White, Black)." },
    { "rank": 7, "severity": "minor", "screenshot": "r4/crop-prim-390-a.png", "observation": "On the phone the buttons stretch to the cell width, so the quiet button's label floats centred in nothing.", "fix": "justify-items-start on the cell." },
    { "rank": 8, "severity": "minor", "screenshot": "r3/hero-390-dark.png", "observation": "The context line ends 'this is 28. Nd2??.' (a glyph then a full stop) and doesn't say who is to move.", "fix": "'You had White, and after 28. Nd2?? it is Black to move.'" },
    { "rank": 9, "severity": "minor", "screenshot": "r3/hero-1440-light.png", "observation": "The scoresheet evaluation '−1.1' sits inside the ring's reach.", "fix": "20px between the ringed word and its evaluation." },
    { "rank": 10, "severity": "minor", "screenshot": "base/foundations-1440-light.png", "observation": "Foundations samples come from Morphy's Opera game, not the fixtures, and the opponent's '?!' is in pen red.", "fix": "Use the middlegame-blunder fixture (rows 28 to 30, 28. Nd2??) and set the opponent's glyph in pencil." }
  ],
  "verdict": "revise"
}
```

Also fixed in this pass:

- `B5b`: the tooltip's key cap used a `→` character. It is now Phosphor `CaretRight`, labelled "Right arrow", and the trigger is an icon button.
- The dialog note claimed "the safe choice comes first", but on a phone the footer stacks the destructive button on top. The note now says only what holds everywhere.
- Foundations values were typed into the page (type sizes, radii, row heights). They are now read from the CSS variables at run time, as are `--shadow-lift` and the spacing. Motion values come from `@ci/ui/motion`, which `motion.test.ts` checks against `tokens.css`.
- `TOK2`: the state grid's rem tracks became `--spacing(n)`.

## Pass 2 (implementer, on r6)

```json
{
  "issues": [
    { "rank": 1, "severity": "minor", "screenshot": "r6/primitives-1440-dark.png", "observation": "The Select trigger shows no hover inside a sheet: it is a well, and the hover ladder resolves to well there.", "fix": "In packages/ui: the trigger's hover should go a step up (sheet) when it already sits on well. Not this task's file; handed to the primitives owner." },
    { "rank": 2, "severity": "minor", "screenshot": "r6/primitives-1440-dark.png", "observation": "The switch has no hover look at all (the Hover cell equals Rest). DESIGN.md doesn't define one.", "fix": "Rule it in DESIGN.md, or leave it; hand to the primitives owner." },
    { "rank": 3, "severity": "minor", "screenshot": "r6/hero-1440-dark.png", "observation": "The hero has no link mark between the scoresheet ring and the board ring, and the graph has no pencil ticks for the other mistakes.", "fix": "Both need component support: a link-mark primitive in @ci/ui/annotations, and a tick-only mark style in EvalGraph. Recorded as follow-ups." },
    { "rank": 4, "severity": "minor", "screenshot": "shots/styleguide/1440-light.png", "observation": "The shots harness captures /styleguide before hydration: empty graph, empty colour values, the hero before its move.", "fix": "scripts/shots.ts should wait for the page to settle (T24 owner). Section shots here wait 2.5s after networkidle." },
    { "rank": 5, "severity": "minor", "screenshot": "r6/primitives-1440-dark.png", "observation": "Select and SegmentedControl offer no disabled or error form, so those cells only explain why.", "fix": "Add the props in packages/ui if a page ever needs them." }
  ],
  "verdict": "pass"
}
```

No blocker or major issue is left. Every remaining item needs a change outside this task's files, and each is handed off in the Task 9 report. The independent critic (a fresh agent with only the PNGs) is still owed. The controller should run it on the `r6/` and `r4open/` shots and append its verdict here.

## Independent critic (controller-run, `critic-t9.md`): needs-changes, 0 Blocker, 7 Important

Fix round 1 answered every Important item. The shots are in scratchpad `t9/fix1/` (sections) and `t9/fix1/open/` (dialog, drawer, menu and toast open), at 390 and 1440, in day and night. `scrollWidth` equals the viewport at 390.

1. **Hover equal to Rest (text field, select, switch).** The components now have a hover:
   - The text field and the select trigger step their edge from `rule-strong` to `ink-faint` (day 4.75:1 on well, night 6.5:1). A disabled or invalid field keeps its own edge.
   - The switch steps its edge when off, and its track from `ink` to `ink-soft` when on. A disabled switch gets no hover.
2. **Disabled tabs equal to Rest.** `disabled:` never matched, because Base UI marks the tab with `data-disabled`. It is now `data-disabled:text-ink-faint`.
3. **Faint night hover.** Inside a night sheet, `--surface-hover` steps up to `sheet` mixed 14% toward `ink`: 1.48:1 against `sheet`, where the old `well` step was 1.17:1. `ink` on it is 8.99:1. Day is unchanged.
4. **"Disabled … no fill" against the primary's well fill.** The intro copy now matches DESIGN.md: only the primary button keeps a quiet well fill.
5. **"Switch to night study" in night.** The item now reads the theme in force ("Switch to day study" at night) and really flips it. Test: `styleguide-overlays.test.tsx`.
6. **Weak night scrim and dialog edge.**
   - The night scrim is `well` at 90%. Ink text behind it drops from 1.74:1 to 1.26:1.
   - Night dialog, drawer, command menu, popover and select list get a 1px `rule-strong` edge: 4.28:1 on the scrim, 3.71:1 against `sheet`.
7. **rgb values wrapping at 390.** Values are `nowrap`, and below 640px the ratio takes its own line under the name.

Polish done:

- Field cells are top-aligned, so the error field lines up.
- Board tints are shown over a light and a dark square.
- No-break space in "46. Qxd6".
- Sections are spaced 64px apart, so the h2 to h3 step reads.
- The overlays layout is balanced: tooltip and popover on the left, "Over the page" on the right.
- The night popover now has an edge (polish 9).
- The scrim copy no longer says "paper".
- `Forced` warns in development when its target is missing (review minor 4).

Polish parked, with reasons:

- **Focus ring on an unselected tab (2):** this is the ruled 2px ink ring at a 2px offset (DESIGN.md "Interaction states"). Changing it is a design-law decision, not a showcase fix.
- **`--dur-draw-small` demo and motion lane widths (4, 5):** both are in `foundations-motion.tsx` (Task 5's 800-line demo), and neither changes a token.
- **Motion intro sentence (6):** already conditional on `useReducedMotion`. It shows only when reduced motion is on.
- **Stray box in the primitives capture (11):** the pinned popover moves during an element screenshot's scroll. In the open-overlay shots it sits in its own card.
- **Touch targets at 390 (12):** the small button already has a 44px coarse-pointer hit area (`touchTarget`, Task 7 e2e). The motion Play buttons belong to Task 5's file.

## Critic re-check: 6 of 7 fixed; Text field and Switch hover still equal to Rest

- **Cause:** a bug in the forcing, not the components. Tailwind writes a stacked variant as one class with escaped colons (`.enabled\:not-aria-invalid\:hover\:border-…`). `forceSelector` matched the `\:hover` inside the class name and rewrote the class itself, so the copied rule never matched the element. The Select hover is a single `hover:` class, so it worked.
- **Fix:** only an unescaped colon starts a pseudo-class (`(?<!\):hover`). Unit test: RED, then GREEN.
- **Too subtle as well:** the edge step to `ink-faint` was close to `rule-strong`. Well controls now step their edge to `ink-soft`: 2.45:1 against the rest edge in day and 2.30:1 at night. When on, the switch track steps from `ink` to `ink-soft`.
- **New guard, `apps/web/e2e/styleguide-states.spec.ts`:** compares the computed border, background, colour and outline of every Hover and Focus cell with Rest, in day and night. It failed on exactly Text field and Switch hover (both themes) with the old forcing, and passes with the fix. It was run against a dev server with a scratch config, because the e2e project's global setup needs the e2e database.
- **Shots:** scratchpad `t9/fix2/` holds the Text field, Switch and Select rows at 390 and 1440, day and night, plus `sheet-1440.png`.
