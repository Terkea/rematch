"use client";

import { Radio } from "@base-ui/react/radio";
import { RadioGroup } from "@base-ui/react/radio-group";
import { useId } from "react";
import { cn } from "../lib/cn.js";
import { motion, spring, useReducedMotion } from "../motion/index.js";
import { touchTarget } from "./touch.js";

export interface SegmentedControlProps {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onValueChange: (value: string) => void;
  className?: string;
}

// A radio group drawn as the Tabs track: a `well` pill, with the chosen option a `sheet` chip in
// `ink` 600 that travels on the `page` spring (transform only), instantly under reduced motion.
// Base UI gives it radio semantics and a roving tab stop: Tab enters on the checked option, and the
// arrow keys move the selection (and so call onValueChange), as a native radio group does.
export function SegmentedControl({
  label,
  value,
  options,
  onValueChange,
  className,
}: SegmentedControlProps) {
  const id = useId();
  const labelId = `${id}-label`;
  const reduced = useReducedMotion();
  return (
    <div data-slot="segmented" className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <span id={labelId} className="w-fit font-sans text-sm font-medium text-ink">
        {label}
      </span>
      <div className="max-w-full overflow-x-auto pointer-coarse:-my-0.5 pointer-coarse:py-0.5">
        <RadioGroup
          aria-labelledby={labelId}
          value={value}
          onValueChange={(v) => {
            if (typeof v === "string") onValueChange(v);
          }}
          className="inline-flex h-10 w-fit items-center gap-0.5 rounded-pill bg-well p-1"
        >
          {options.map((o) => {
            const checked = o.value === value;
            return (
              <Radio.Root
                key={o.value}
                value={o.value}
                className={cn(
                  "relative isolate inline-flex h-8 shrink-0 cursor-pointer items-center justify-center rounded-pill px-3.5",
                  "font-sans text-sm font-medium whitespace-nowrap text-ink-soft select-none",
                  "transition-colors duration-(--dur-quick) ease-out hover:text-ink",
                  "data-checked:font-semibold data-checked:text-ink",
                  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
                  touchTarget,
                )}
              >
                {checked ? (
                  <motion.span
                    layoutId={`${id}-chip`}
                    transition={reduced ? { duration: 0 } : spring.page}
                    aria-hidden="true"
                    className="absolute inset-0 -z-10 rounded-pill bg-sheet inset-ring-1 inset-ring-rule"
                  />
                ) : null}
                {/* An invisible 600-weight copy keeps every option as wide as its chosen self. */}
                <span className="inline-grid">
                  <span
                    aria-hidden="true"
                    className="invisible col-start-1 row-start-1 font-semibold"
                  >
                    {o.label}
                  </span>
                  <span className="col-start-1 row-start-1 text-center">{o.label}</span>
                </span>
              </Radio.Root>
            );
          })}
        </RadioGroup>
      </div>
    </div>
  );
}
