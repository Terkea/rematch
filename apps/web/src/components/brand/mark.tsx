import { REMATCH_MARKS } from "./marks.generated";

export type MarkVariant = keyof typeof REMATCH_MARKS;

/**
 * The boy with his board and his dad's hand, in the current text colour (ink by day, cream at
 * night; never pen red). `mid` is drawn for 40-64 px (headers, the footer, empty states), `full`
 * for 96 px and up (sign-in, the 404). Always decorative: whatever it sits in carries the name.
 * Size it by height (`h-*`); the width follows the drawing.
 */
export function BrandMark({ variant, className }: { variant: MarkVariant; className: string }) {
  const mark = REMATCH_MARKS[variant];
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      data-brand-mark={variant}
      viewBox={mark.viewBox}
      className={`w-auto shrink-0 ${className}`}
      // A constant generated from the committed logo (scripts/brand-assets.ts), never user data.
      // biome-ignore lint/security/noDangerouslySetInnerHtml: the generated logo paths
      dangerouslySetInnerHTML={{ __html: mark.body }}
    />
  );
}
