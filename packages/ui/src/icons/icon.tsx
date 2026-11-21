import type { Icon as PhosphorIcon } from "@phosphor-icons/react";

export type { PhosphorIcon };

export interface IconProps {
  icon: PhosphorIcon;
  /** The accessible name. Without it the icon is decorative and hidden from assistive tech. */
  label?: string;
  /** 20 in UI controls, 16 inline with text (DESIGN.md, "Icons"). */
  size?: 16 | 20 | 24;
  weight?: "regular" | "bold" | "fill";
  className?: string;
}

/** A Phosphor icon in `currentColor`. Never Lucide or emoji (B13). */
export function Icon({ icon: Glyph, label, size = 20, weight = "regular", className }: IconProps) {
  return label ? (
    <Glyph size={size} weight={weight} role="img" aria-label={label} className={className} />
  ) : (
    <Glyph size={size} weight={weight} aria-hidden="true" focusable="false" className={className} />
  );
}
