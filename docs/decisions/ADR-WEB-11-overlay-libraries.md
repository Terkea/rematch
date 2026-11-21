# ADR-WEB-11. Overlay libraries: sonner, vaul and cmdk

Date: 2026-11-21
Status: Accepted

## Context

The design system needs a toast, a bottom drawer with drag-to-dismiss, and a command menu. T7 built dialog, popover and tooltip on Base UI. Writing swipe physics, a toast queue and fuzzy list navigation by hand would cost more than it earns, and the owner wants these to feel physical.

## Decision

- **sonner** for toasts, **vaul** for the drawer and **cmdk** for the command list, all restyled to the tokens (`sheet`, `--radius-sheet`, `--shadow-lift`, `paper` wash, no black scrim or blur).
- **Two dialog stacks.** vaul uses Radix Dialog; the command menu is cmdk's list inside a Base UI dialog (not cmdk's own Radix `Command.Dialog`), so only the drawer is Radix. This keeps one stack for the dialog, popover, tooltip and menu, and Radix only where vaul forces it.
- **Focus-scope risk.** A Radix drawer opened from a Base UI command menu has two focus traps and two "outside" layers: each treats the other as outside and can fight over focus or dismiss on the other's pointer events. Rule: a menu item that opens a drawer must close the menu first (`onSelect` closes, then opens on the next frame), never stack them. Re-test this when the first such item ships.
- **Hand-rolled focus return.** Ctrl+K opens the menu with no trigger element, so Base UI has nothing to return focus to. `CommandMenu` remembers `document.activeElement` on open and restores it on close. Ctrl+K is ignored in text inputs, textareas, selects and contenteditable unless the menu is already open.
- **Toasts use `unstyled: true`.** Sonner's own CSS is stock-looking and cannot take our tokens, so every part is styled through `classNames`. Colour meaning follows DESIGN.md "Semantic states": `pen-text` for errors, `good-text` for success, each with a Phosphor icon from `@ci/ui/icons`. Sonner has one polite live region, so `toast.error` wraps its message in `role="alert"` to be assertive. The dismiss button is 44px on coarse pointers via `touchTarget`.
- **Positioning.** Sonner takes one `position` and has no CSS-only responsive form, so the side flips from bottom-centre to bottom-right in JS after hydration at 640px. No toast exists at first paint, so nothing visibly jumps. `<Toaster />` mounts once, in the root layout.
- **Reduced motion** (M10): drawer travel and toast transitions are cut to `--dur-instant`; the menu only fades over `--dur-quick`.

## Consequences

- Radix ships in the bundle through vaul only. Revisit if Base UI gains a drawer.
- vaul unmounts on `transitionend`, which jsdom never fires, so unit tests assert `data-state="closed"`. Unmount needs a browser test.
