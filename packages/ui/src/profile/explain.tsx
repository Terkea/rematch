"use client";

import { Icon, InfoIcon } from "../icons/index.js";
import { cn } from "../lib/index.js";
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverTitle,
  PopoverTrigger,
} from "../primitives/popover.js";
import { touchTarget } from "../primitives/touch.js";

/**
 * One glossary entry as the UI shows it: the host passes its own glossary's text (apps/web's
 * `server/profile/glossary.ts`); this package never writes a definition of its own.
 */
export interface ExplainEntry {
  name: string;
  /** One sentence: what it means at the board. */
  meaning: string;
  measured: string;
  whyItMatters: string;
  tip: string;
}

/** "Not winning won games" to "not winning won games"; "KPI" and "Win%" keep their capitals. */
function inSentence(name: string): string {
  const second = name.charAt(1);
  return second && second === second.toUpperCase() && /[A-Z]/.test(second)
    ? name
    : name.charAt(0).toLowerCase() + name.slice(1);
}

const PARTS = [
  ["measured", "How it's measured"],
  ["whyItMatters", "Why it matters"],
  ["tip", "What to try"],
] as const;

/**
 * An "i" beside a name that opens the term's explanation: what it means, how it is measured, why
 * it matters and one thing to try. A button (`aria-expanded`), opened by click or tap, never by
 * hover alone; Escape or a click outside closes it and focus returns to the trigger (Base UI's
 * popover). The popup is the `sheet` popover with `--shadow-lift`, and grows from the trigger over
 * `--dur-quick` (M9), fading only under reduced motion (M10).
 */
export function ExplainPopover({ entry, className }: { entry: ExplainEntry; className?: string }) {
  return (
    <Popover>
      <PopoverTrigger
        aria-label={`What is ${inSentence(entry.name)}?`}
        className={cn(
          "relative inline-flex size-6 shrink-0 items-center justify-center rounded-full align-middle text-ink-faint",
          "transition-colors duration-(--dur-quick) ease-out hover:text-ink data-popup-open:text-ink",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
          touchTarget,
          className,
        )}
      >
        <Icon icon={InfoIcon} size={16} />
      </PopoverTrigger>
      <PopoverContent className="w-80 gap-3">
        <PopoverTitle className="font-serif text-lg font-semibold">{entry.name}</PopoverTitle>
        <PopoverDescription className="m-0 text-base text-ink">{entry.meaning}</PopoverDescription>
        <dl className="m-0 flex flex-col gap-3">
          {PARTS.map(([key, title]) => (
            <div key={key} className="flex flex-col gap-0.5">
              <dt className="text-sm font-medium text-ink">{title}</dt>
              <dd className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">
                {entry[key]}
              </dd>
            </div>
          ))}
        </dl>
      </PopoverContent>
    </Popover>
  );
}

/** The definition line: the entry's one-sentence meaning in muted text, under a name. */
export function Meaning({ entry, className }: { entry: ExplainEntry; className?: string }) {
  return <p className={cn("m-0 max-w-prose text-sm text-ink-soft", className)}>{entry.meaning}</p>;
}
