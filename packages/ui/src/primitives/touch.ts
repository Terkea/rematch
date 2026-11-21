// WCAG 2.5.5 / Apple HIG: at least a 44px target on touch screens. On a coarse pointer the element's
// ::after grows to max(its own size, 44px), centred, so the visual size stays the same and only the
// hit area grows. The element needs `relative`. Fine pointers are unaffected.
//
// The area reaches (44 - height) / 2 past the control: 2px for an md button (40px), 6px for an sm
// button or a tab (32px). So stack md controls at least 4px apart and sm controls at least 12px
// apart, or the later control's area covers the earlier one's (DESIGN.md, "Touch targets"). A
// scrolling ancestor clips the area too, so it needs that much padding (see TabsList).
// apps/web/e2e/touch-targets.spec.ts proves it with elementFromPoint.
export const touchTarget = [
  "pointer-coarse:after:absolute",
  "pointer-coarse:after:top-1/2",
  "pointer-coarse:after:left-1/2",
  "pointer-coarse:after:size-full",
  "pointer-coarse:after:min-h-11",
  "pointer-coarse:after:min-w-11",
  "pointer-coarse:after:-translate-x-1/2",
  "pointer-coarse:after:-translate-y-1/2",
].join(" ");
