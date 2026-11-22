import { type ClassValue, clsx } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

// tokens.css replaces Tailwind's scales (B14), so tailwind-merge must be told the token font
// sizes: otherwise `text-prose` reads as a colour and `cn("text-prose", "text-ink")` drops it
// (M5b T9 found it in SegmentText). Keep this list in step with the `--text-*` tokens.
const twMerge = extendTailwindMerge({
  extend: {
    theme: { text: ["2xs", "xs", "sm", "base", "prose", "lg", "xl", "2xl", "3xl", "4xl", "5xl"] },
  },
});

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
