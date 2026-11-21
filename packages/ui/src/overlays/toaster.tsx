"use client";

import { type ComponentProps, type CSSProperties, useSyncExternalStore } from "react";
import { Toaster as Sonner, toast } from "sonner";
import { cn } from "../lib/cn.js";

// Sonner, restyled to the scoresheet: a `sheet` slip with a `rule-strong` hairline, lifted by the
// one shadow because it floats. Stock sonner colours, rich-colour variants and icons are off; a
// toast names the finished action in `ink` (DESIGN.md, "Semantic states"). Bottom-centre below
// 640px, bottom-right above. Sonner animates transform and opacity; under reduced motion its
// transitions are cut to the instant token (M10).

export { toast };

const WIDE = "(min-width: 640px)";

function subscribe(onChange: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return () => {};
  const media = window.matchMedia(WIDE);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

function useWide(): boolean {
  return useSyncExternalStore(
    subscribe,
    () =>
      typeof window !== "undefined" && typeof window.matchMedia === "function"
        ? window.matchMedia(WIDE).matches
        : false,
    () => false,
  );
}

export function Toaster({ className, ...props }: ComponentProps<typeof Sonner>) {
  const wide = useWide();
  return (
    <Sonner
      position={wide ? "bottom-right" : "bottom-center"}
      offset={{ bottom: 16, right: 16, left: 16 }}
      mobileOffset={{ bottom: 12, left: 16, right: 16 }}
      gap={8}
      visibleToasts={3}
      className={cn(
        "ci-toaster motion-reduce:[&_[data-sonner-toast]]:!duration-(--dur-instant)",
        className,
      )}
      style={
        {
          "--width": "min(356px, calc(100vw - 2 * var(--space-gutter)))",
        } as CSSProperties
      }
      toastOptions={{
        unstyled: true,
        classNames: {
          toast: cn(
            "pointer-events-auto flex w-full items-center gap-3 rounded-sheet border border-rule-strong bg-sheet",
            "px-4 py-3 font-sans text-sm text-ink shadow-lift",
          ),
          title: "font-medium text-ink",
          description: "text-ink-soft",
          actionButton: cn(
            "relative ml-auto inline-flex h-8 items-center rounded-field bg-ink px-3 text-sm font-medium text-on-ink",
            "pointer-coarse:after:absolute pointer-coarse:after:top-1/2 pointer-coarse:after:left-1/2 pointer-coarse:after:size-full",
            "pointer-coarse:after:min-h-11 pointer-coarse:after:min-w-11 pointer-coarse:after:-translate-x-1/2 pointer-coarse:after:-translate-y-1/2",
            "hover:bg-ink-soft focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
          ),
          closeButton: "rounded-field border border-rule-strong bg-sheet text-ink-soft",
        },
      }}
      {...props}
    />
  );
}
