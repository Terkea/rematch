"use client";

import {
  type ComponentProps,
  type CSSProperties,
  type ReactNode,
  useSyncExternalStore,
} from "react";
import { Toaster as Sonner, toast as sonnerToast } from "sonner";
import { Icon } from "../icons/icon.js";
import { CheckCircleIcon, WarningCircleIcon, XIcon } from "../icons/index.js";
import { cn } from "../lib/cn.js";
import { touchTarget } from "../primitives/touch.js";

// Sonner, restyled to the scoresheet: a `sheet` slip with a `rule-strong` hairline, lifted by the
// one shadow because it floats. Stock sonner colours, rich-colour variants and icons are off; a
// toast names the finished action in `ink` (DESIGN.md, "Semantic states"). Bottom-centre below
// 640px, bottom-right above. Sonner animates transform and opacity; under reduced motion its
// transitions are cut to the instant token (M10).

// Sonner has one polite live region for everything. An error must interrupt, so `toast.error`
// wraps its message in `role="alert"` (implicitly assertive); every other toast stays polite.
// Mount `<Toaster />` ONCE, in the root layout: a second one renders every toast twice.
export const toast = Object.assign(
  (...args: Parameters<typeof sonnerToast>) => sonnerToast(...args),
  sonnerToast,
  {
    error: (message: ReactNode, data?: Parameters<typeof sonnerToast.error>[1]) =>
      sonnerToast.error(<span role="alert">{message}</span>, data),
  },
);

// Sonner takes one `position` and has no CSS-only responsive form, so the side is chosen in JS:
// the server render and first client paint use bottom-centre, and a wide viewport flips to
// bottom-right right after hydration. No toast exists at first paint, so nothing visibly jumps.
// Offsets use the gutter token, and the width never exceeds the viewport minus both gutters.
const WIDE = "(min-width: 640px)";
const GUTTER = "var(--space-gutter)";

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
      offset={{ bottom: GUTTER, right: GUTTER, left: GUTTER }}
      mobileOffset={{ bottom: GUTTER, right: GUTTER, left: GUTTER }}
      closeButton
      gap={8}
      visibleToasts={3}
      className={cn(
        "ci-toaster motion-reduce:[&_[data-sonner-toast]]:!duration-(--dur-instant)",
        className,
      )}
      style={
        {
          "--width": `min(22rem, calc(100vw - 2 * ${GUTTER}))`,
        } as CSSProperties
      }
      icons={{
        success: <Icon icon={CheckCircleIcon} />,
        error: <Icon icon={WarningCircleIcon} />,
        close: <Icon icon={XIcon} size={16} />,
      }}
      toastOptions={{
        unstyled: true,
        closeButtonAriaLabel: "Dismiss",
        classNames: {
          toast: cn(
            "relative flex w-full items-center gap-3 rounded-sheet border border-rule-strong bg-sheet",
            "py-3 pr-12 pl-4 font-sans text-sm text-ink shadow-lift",
          ),
          icon: "flex shrink-0 items-center",
          title: "font-medium",
          description: "text-ink-soft",
          // DESIGN.md, "Semantic states": red text is pen-text, green text is good-text.
          error: "text-pen-text",
          success: "text-good-text",
          closeButton: cn(
            "absolute top-1/2 right-2 inline-flex size-8 -translate-y-1/2 items-center justify-center rounded-field text-ink-soft",
            "transition-colors duration-(--dur-quick) ease-out hover:bg-(--surface-hover) hover:text-ink",
            touchTarget,
            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
          ),
        },
      }}
      {...props}
    />
  );
}
