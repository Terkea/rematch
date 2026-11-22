"use client";

import { ease, motion, useReducedMotion } from "@ci/ui/motion";
import { Button, type ButtonProps } from "@ci/ui/primitives";
import type { MouseEvent } from "react";

export interface PendingButtonProps extends ButtonProps {
  /** While true the button keeps focus but ignores presses, and names the work in progress. */
  pending: boolean;
  pendingLabel: string;
  /** Unavailable for now (a cooldown, say): focusable and announced, but presses do nothing. */
  unavailable?: boolean;
}

/**
 * A button that shows its own async state instead of a blocking spinner (B18): the label names
 * the work ("Sending link") and an ink line soaks along its foot like the progress fill
 * (DESIGN.md, choreography 7, "Progress"). Under reduced motion the line is simply there (M10).
 */
export function PendingButton({
  pending,
  pendingLabel,
  unavailable = false,
  children,
  onClick,
  className,
  variant = "primary",
  ...props
}: PendingButtonProps) {
  const reduced = useReducedMotion();
  const blocked = pending || unavailable;
  const tone = variant === "primary" ? "bg-on-ink" : "bg-ink";
  return (
    <Button
      {...props}
      variant={variant}
      aria-disabled={blocked ? true : undefined}
      onClick={(event: MouseEvent<HTMLElement>) => {
        if (blocked) {
          event.preventDefault();
          return;
        }
        onClick?.(event);
      }}
      className={[
        "overflow-hidden",
        // DESIGN.md "Interaction states", disabled: well fill (primary) or a rule edge, ink-faint.
        unavailable && !pending
          ? variant === "primary"
            ? "border-well bg-well text-ink-faint"
            : "border-rule text-ink-faint"
          : "",
        className ?? "",
      ].join(" ")}
    >
      {pending ? pendingLabel : children}
      {pending ? (
        <motion.span
          aria-hidden="true"
          className={`absolute inset-x-0 bottom-0 h-0.5 origin-left opacity-40 ${tone}`}
          initial={reduced ? false : { scaleX: 0 }}
          animate={{ scaleX: reduced ? 1 : 0.92 }}
          transition={{ duration: 2.4, ease: [...ease.soak] }}
        />
      ) : null}
    </Button>
  );
}
