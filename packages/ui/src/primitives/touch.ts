// WCAG 2.5.5 / Apple HIG: at least a 44px target on touch screens. On a coarse pointer the element's
// ::after grows to max(its own size, 44px), centred, so the visual size stays the same and only the
// hit area grows. The element needs `relative`. Fine pointers are unaffected.
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
