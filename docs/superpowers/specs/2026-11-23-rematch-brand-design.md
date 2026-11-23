# Rematch brand: a kid at the board, learning from his dad

Status: approved by the owner, 2026-11-23. The logo was chosen the same day: the boy with his board and his dad's hand, ink only (`packages/ui/assets/brand`).

## Intent

Rematch is built in memory of the maintainer's dad, who played chess all his life. The brand is a child at the board learning from his father. The app plays the dad: it sits next to you, marks the mistake you keep making in red pen, and asks you to try it again. The logo is drawn from a photo of the dad as a small boy holding his chess board.

## Goals

- The app feels warm and personal, like a dad teaching his kid, not like a dashboard.
- The coach (report, chat, weekly review) speaks in a fatherly voice: patient, plain, honest, never preachy.
- One identity across the app, site, README, video and favicon.

## Non-goals

- No change to the data, the statistics, the validator, the 80% ranges or the trainer logic.
- No childish UI: no cartoon type, no rounded "kids app" look on data screens.

## Anti-goals (a failure even if it ships)

- Warmth that costs trust: a fatherly line that states something the validator can't cite.
- Sentimentality: using the dad story inside the app's copy. It stays in the README section the owner approved, nowhere else unless the owner approves the words.
- Red used for anything other than "dad's pen" on a mistake.

## Approaches considered

1. **Warm the brand, keep the analysis precise (chosen).** New mark and wordmark, a palette warmed from the photo, pencil and crayon touches only in soft places (empty states, the landing page, celebrations), and a fatherly coach voice. Boards, graphs and numbers stay crisp. This keeps the honest-numbers promise and changes the fewest screens.
2. **A full childlike restyle** (hand-drawn everything, playful type). Rejected: it fights the trust the numbers need, and every screen and baseline changes.
3. **Only the outside** (logo, site, README, video). Rejected: the app would not match the story the site tells.

## The design

**Mark.** The boy with his board, from the photo, drawn by the designer agent: the primary mark, a variant with dad's hand on his shoulder in red pen, a simplified favicon mark and a lockup. The owner picks the final one.

**Palette.** "Annotated scoresheet" keeps its structure and moves warmer. These are proposed values; the DESIGN.md contrast checks decide the final ones.

| Token | Day now | Day proposed | Role |
|---|---|---|---|
| `--color-paper` | `#F3F5F8` | `#F4EFE6` | faded photo paper |
| `--color-sheet` | `#FBFCFD` | `#FBF8F2` | the scoresheet |
| ink | `#16233B` | `#1F2A3D` | text |
| `--color-pen` | `#D3312B` | `#C8372D` | dad's red pen, mistakes only |
| good-move green | `#24684B` | `#2F6B4F` | better moves |
| pencil | `#56657F` | `#6B6458` | warm graphite |
| crayon (new) | — | `#E3A92F` | celebrations only, never data |

Night study moves warmer in the same way: a desk lamp on warm paper, not a blue screen.

DESIGN.md's ban "paper is never cream" is replaced by "paper is faded, warm and photographic, never yellow".

**Type.**
- Literata and Schibsted Grotesk stay for all reading and data.
- The handwritten face is used only for dad's margin notes (short annotations beside a marked move, like "same knight again") and the wordmark, if the designer's lockup uses it.

**Hand-made touches.** Pencil textures and hand-drawn rings only in:
- empty states;
- the landing page;
- a drill solved ("there you go");
- milestones.

Never on charts or tables.

**The coach's voice.** "Dad at the board."
- It speaks to you directly, short and plain: "You were winning here. Then the knight went to d2 and left your queen with nobody watching her." "Let's try this one again."
- It praises effort and progress honestly, and never flatters.
- It keeps every validator rule.
- It's implemented as new prompt versions: the tone prompt, report, chat and weekly review, keeping the old versions for stored hashes.
- The app's own coach-voice copy (empty states, trainer feedback, focus card) gets the same voice.
- If the owner shares how their dad talked about chess, a few of his habits of speech shape it. Otherwise the voice is general.

**Where it lands.**
- `packages/ui/DESIGN.md` and `tokens.css`;
- the app shell (wordmark, favicon set, social card);
- empty states and celebrations;
- the AI prompts;
- the site theme and README header;
- the video's title card, which needs a re-render with the new mark.

## Lanes once approved (in parallel)

1. **Tokens and DESIGN.md:**
   - the palette and night theme, the bans, and the handwritten-note component;
   - contrast checks, `design:check` and visual baselines.
2. **The coach's voice:**
   - the prompt versions plus the validator tests unchanged and green;
   - the app's coach copy.
3. **Brand rollout after the logo is picked:**
   - the favicon set, the app header, the site, README and the social card;
   - the video title card.

## Left to the builder

Exact token values after contrast checks, the pencil texture technique (SVG filter or image), and the handwritten face (it must cover Romanian diacritics, for a possible future translation).
