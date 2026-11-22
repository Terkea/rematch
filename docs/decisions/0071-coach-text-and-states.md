# 0071. Coach text, disclosure and states in `@ci/ui`

Date: 2026-11-22. Status: accepted (M5b Task 3). Builds on ADR 0060 (model text is stored as
segments), spec §14 (AI disclosure), DESIGN.md ("Severity and marks", "Type", "Semantic states",
B18).

## Context

Every coach surface of M5b (report, weekly review, weakness notes, the chat) shows text written
by a model. The worker stores that text as resolved `segments` (Rust `ai::segments::Segment`) and
a stored `ai_disclosure`. The web must render both the same way everywhere, must never parse
`{{…}}` tokens, must show that the text is AI-written, and must show work in progress without a
spinner. `@ci/ui` must not import from the web app.

## Decision

`@ci/ui/coach` holds five components and a structural copy of `Segment` (`types.ts`):

- **Segment rendering (`SegmentText`).** `text` renders as text. `move`, `pv`, `line` and
  `square` render in the notation face (Literata 600, lining tabular figures). `best` adds the
  good mark: `good-text` colour, the solid good arrowhead drawn inline (shape as well as colour),
  and "better move" for screen readers. `num` renders the server's `display` string; without one,
  a finite `value` in en-GB, otherwise nothing. `date` renders a `<time dateTime>` in UTC at day
  precision ("12 Nov 2026"); an unreadable date renders nothing. `eng` renders its text with a
  dashed `pencil` underline and a visible "engine check" label, as a `note` named
  "Engine check: …".
- **The link rule.** A `move`, `best`, `pv` or `line` segment is a link exactly when the host's
  `hrefFor` returns a URL (the web's `segmentHref`). `@ci/ui` builds no URLs itself; a throwing
  `hrefFor` is treated as no link.
- **Unknown kinds.** A segment whose `kind` this build does not know (or a malformed one) renders
  nothing and never throws, so a newer worker never breaks an older page. No `NaN`, `Infinity`,
  `undefined` or `null` text can come out of a segment.
- **Disclosure.** `AiDisclosure` shows the stored label (a quiet `well` pill), the stored note and
  "Written on 12 Nov 2026"; `compact` keeps the note behind an info tooltip and in screen-reader
  text. While an answer streams and no disclosure is stored yet, the message shows the fixed
  label (`AI_LABEL`, the Rust `AI_DISCLOSURE_LABEL`). A done message with no stored disclosure
  (fixed server copy) shows no label.
- **States (`CoachState`).** `building` and `writing` are a title and body over a
  `ProgressPlaceholder` (shape `list`), never a spinner (B18). `failed` and `budget` use the
  warning treatment (ink on `well`, `rule-strong` edge, Warning or Clock icon), `unavailable` and
  `computed_only` the info treatment, `empty` a plain note; red is never used, because none of
  these is the player's mistake.
- **Placeholder copy.** An assistant message that is `pending` with nothing streamed shows
  "Your coach is reading your games" as a `writing` placeholder. A streaming message adds "Your
  coach is still writing" under its text; a failed message without server copy says "This answer
  could not be written. Ask again in a minute."
- **Chat.** Streamed chunks render in `seq` order; consecutive answer chunks join into one
  paragraph, and an interim chunk is a `pencil` aside (a dashed pencil ring, `ink-soft` text).
  The streamed block settles in on the `pen` spring on mount (reduced motion: instant). A failed
  answer hides any partial stream, which was never validated. The message is `aria-busy` while
  pending or streaming, inside a polite live region, so assistive tech reads the answer once.
  Engine checks read "Engine check: 1. e4 e5 2. Nf3 (done)", counted from move 1. The composer
  counts code points as the server does, shows the count from 200 characters left ("1 character
  over" in `pen-text`, a form error), sends on Enter, breaks on Shift+Enter, refuses empty or
  over-limit text, ignores a second send while one is in flight, keeps the text when `onSubmit`
  returns false, and shows `disabledReason` in place of the send button.

## Consequences

- Every coach page renders model text through one path, so the honesty and token rules are
  enforced in one place and pinned by `packages/ui/test/coach.test.tsx`.
- The component prop is named `role` as the plan's interface says; Biome's
  `useValidAriaRole` flags a literal `role="user"` on the component, so callers pass it through a
  spread or a variable.
- The compact disclosure's tooltip does not open on touch; the label and date stay visible and
  the note stays in the accessible text. A surface that must show the note on phones uses the
  full form.
