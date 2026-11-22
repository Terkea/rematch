# Components: which one, and its states

All from `@ci/ui/<folder>`; read the component file for its props. Never hand-roll what exists, and never import Base UI, Sonner, Vaul or cmdk directly in the app.

## Primitives (`@ci/ui/primitives`, `packages/ui/src/primitives/`)

| Use | For |
|---|---|
| `Button` | every action. `variant`: `primary` (ink fill, one per view), `outline`, `quiet` (toolbar and inline), `danger` (ink-outlined, not red). `size`: `sm`, `md`. `asChild` for links. Has the pen-press. |
| `Input`, `Field` | text entry. `Field` carries label, hint and error (`pen-text` with `WarningCircle`, `aria-invalid`). |
| `Select`, `SegmentedControl`, `Switch`, `Tabs` | choose one of many, a mode, an on/off setting, sibling views. |
| `Dialog` | a decision that blocks. `Popover` and `Tooltip` (one `TooltipProvider`) for floating help; `Kbd` for key hints. |

Every interactive element meets `touchTarget`, has a visible focus ring (2px `ink`, offset 2px) and a disabled state (`ink-faint`, `aria-disabled`).

## Overlays (`@ci/ui/overlays`)

- `toast(...)`, with one `Toaster` in the root layout (a second doubles every toast). A toast names the finished action: "chess.com connected", "FEN copied". Failures use `toast.error` (assertive).
- `Drawer` (phone sheets), `CommandMenu` (the palette).

## Icons

`Icon` (`@ci/ui/icons`) takes a Phosphor icon: `<Icon icon={CopyIcon} size={16} />`. Size 16 inline with text or in a `sm` button, 20 in controls, 24 only for large targets. Decorative by default; pass `label` only when the icon stands alone without text.

Picking: use the existing curated set in `packages/ui/src/icons/index.ts` first. If the icon you need is missing, add its `...Icon` export to that list (alphabetical) from `@phosphor-icons/react/dist/ssr`. That needs no ruling when it comes from the Phosphor set, regular weight, at the sizes above. A different icon library, a custom drawing or an emoji does (B13).

Usual mappings: copy `CopyIcon`; external link `ArrowSquareOutIcon`; close `XIcon`; confirm `CheckIcon`; delete `TrashIcon`; settings `GearSixIcon`; search `MagnifyingGlassIcon`; add `PlusIcon`; retry `ArrowCounterClockwiseIcon`; sync `ArrowsClockwiseIcon`; warning `WarningIcon`; error `WarningCircleIcon`; success `CheckCircleIcon`; info `InfoIcon`; menu `ListIcon`; sound `SpeakerHighIcon` or `SpeakerSlashIcon`. Add those not yet exported (copy, external link) when first needed. An icon never replaces a button's text for a primary action, and never carries meaning alone.

## Failures and toasts

A finished action toasts its name ("FEN copied"). A failure uses `toast.error` and stays short, honest and actionable, naming what failed and the fix, with no apology and no invented cause:

- clipboard denied or unavailable: `toast.error("Couldn't copy the FEN. Select it and copy by hand.")` (and keep the text visible so that works);
- network: "Couldn't reach the server. Check your connection and try again.";
- other: say what happened if known, else "Something went wrong. Try again.", and never "failed successfully" or a raw error message.

Never toast success before the action resolved: await the clipboard or request, then toast.

## Loading, empty, error, long content

Every surface that fetches defines all four:

- **Loading:** `ProgressPlaceholder` (`@ci/ui/progress`) with a `shape` (`graph`, `stat`, `list`, `board`), a `verb` ("Analysing" or "Importing") and real counts or none. No spinner (B18). A filter change keeps the old content at reduced opacity with `aria-busy`. A route's `loading.tsx` shows the ruled shape.
- **Empty:** says why and offers the next step ("chess.com name has no games yet", a link to Accounts), never a blank.
- **Error:** what happened and how to fix it, no apology; `Field` error for forms, `toast.error` for actions.
- **Long content:** wrap or truncate with intent. Check 360px: no horizontal scroll. Long game lists, 300-ply reviews and long names are in `@ci/ui/fixtures`.

## Data display

- `KpiStat`, `Stat`, `Sparkline` (`@ci/ui/stats`): a rate with its basis, interval and sparkline (DESIGN.md "KPI stats", ADR-WEB-12). Never a stat banner (B9).
- `AccuracyTrend`, `JudgementColumns`, `ResultSplit` (`@ci/ui/charts`), `WeaknessTimeline` (`@ci/ui/timeline`), `AreaScore` (`@ci/ui/area-score`): encodings in DESIGN.md "Chart series" and ADR-WEB-14. Series are tonal ink with dashes, never a rainbow; red and green keep their chess meanings. Each has a text alternative and a keyboard path.
- `Scoresheet`, `ReviewControls`, `ClockPair` (`@ci/ui/review`): the game review.
- `Board`, `EvalBar`, `EvalGraph` and annotations: the `chess-ui` skill.

## Styleguide

Every reusable component has a section in `apps/web/src/app/styleguide/sections/`. Add yours there, forcing its states where useful (`state-grid.tsx`).
