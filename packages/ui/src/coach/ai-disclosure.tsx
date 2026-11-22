"use client";

import { Icon, InfoIcon } from "../icons/index.js";
import { cn } from "../lib/index.js";
import { Tooltip, TooltipContent, TooltipTrigger } from "../primitives/tooltip.js";
import { touchTarget } from "../primitives/touch.js";
import { dayOf } from "./segment-text.js";
import { AI_LABEL, type AiDisclosureData } from "./types.js";

// Every surface that shows model text shows its stored disclosure (spec §14): the label, the note
// and the day it was written. The label is a quiet `well` pill, the same treatment as "Early
// estimate", so it reads as a fact about the text, not as a badge (B8).

const pill =
  "inline-flex items-center rounded-pill bg-well px-2.5 py-0.5 font-sans text-sm font-medium text-ink-soft";

/** The label alone, for text that is still streaming and has no stored disclosure yet. */
export function AiLabel({ label = AI_LABEL }: { label?: string }) {
  return (
    <span data-ai-label className={pill}>
      {label}
    </span>
  );
}

export interface AiDisclosureProps {
  disclosure: AiDisclosureData;
  /** Label and date on one line, with the note behind an info tooltip (and kept for screen readers). */
  compact?: boolean;
  className?: string;
}

export function AiDisclosure({ disclosure, compact = false, className }: AiDisclosureProps) {
  const label = disclosure.label?.trim() ? disclosure.label : AI_LABEL;
  const note = disclosure.note?.trim() ? disclosure.note : null;
  const day = dayOf(disclosure.generatedAt);
  const written = day ? (
    <span className="font-sans text-sm text-ink-faint">
      Written on <time dateTime={day.iso}>{day.text}</time>
    </span>
  ) : null;

  if (compact) {
    return (
      <div
        data-ai-disclosure="compact"
        className={cn("flex flex-wrap items-center gap-x-3 gap-y-1", className)}
      >
        <AiLabel label={label} />
        {written}
        {note ? (
          <>
            <Tooltip>
              <TooltipTrigger
                render={
                  <button
                    type="button"
                    aria-label="About this AI text"
                    className={cn(
                      "relative -my-1.5 inline-flex size-8 items-center justify-center rounded-field text-ink-faint",
                      "transition-colors duration-(--dur-quick) ease-out hover:bg-(--surface-hover) hover:text-ink",
                      "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
                      touchTarget,
                    )}
                  />
                }
              >
                <Icon icon={InfoIcon} size={16} />
              </TooltipTrigger>
              <TooltipContent className="whitespace-normal">{note}</TooltipContent>
            </Tooltip>
            <span className="sr-only">{note}</span>
          </>
        ) : null}
      </div>
    );
  }

  return (
    <div data-ai-disclosure="full" className={cn("flex flex-col gap-1.5", className)}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <AiLabel label={label} />
        {written}
      </div>
      {note ? <p className="m-0 max-w-prose font-sans text-sm text-ink-soft">{note}</p> : null}
    </div>
  );
}
