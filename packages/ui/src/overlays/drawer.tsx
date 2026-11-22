"use client";

import type { ComponentProps } from "react";
import { Drawer as VaulDrawer } from "vaul";
import { cn } from "../lib/cn.js";

// Vaul bottom sheet: a `sheet` panel, `--radius-sheet` on the top corners only, lifted by the one
// shadow, over a `paper` wash (no black scrim, B7). It travels over `--dur-base` on transform
// (vaul's own drag stays physical); under reduced motion the travel is cut to the instant token
// (M10). Focus is trapped, Escape closes, and focus returns to the trigger.

function Overlay({ className, ...props }: ComponentProps<typeof VaulDrawer.Overlay>) {
  return (
    <VaulDrawer.Overlay
      data-slot="drawer-overlay"
      className={cn("fixed inset-0 z-50 bg-paper/80 dark:bg-well/90", className)}
      {...props}
    />
  );
}

function Content({ className, children, ...props }: ComponentProps<typeof VaulDrawer.Content>) {
  return (
    <VaulDrawer.Portal>
      <Overlay />
      <VaulDrawer.Content
        data-slot="drawer-content"
        className={cn(
          "fixed inset-x-0 bottom-0 z-50 mx-auto flex max-h-[85dvh] w-full max-w-lg flex-col gap-2",
          "rounded-t-sheet border-t border-transparent bg-sheet p-(--space-panel-pad) pt-3 text-ink shadow-lift outline-none dark:border-rule-strong",
          "!duration-(--dur-base) motion-reduce:!duration-(--dur-instant)",
          className,
        )}
        {...props}
      >
        <div aria-hidden="true" className="mx-auto mb-2 h-1 w-10 rounded-pill bg-rule-strong" />
        {children}
      </VaulDrawer.Content>
    </VaulDrawer.Portal>
  );
}

function Title({ className, ...props }: ComponentProps<typeof VaulDrawer.Title>) {
  return (
    <VaulDrawer.Title
      data-slot="drawer-title"
      className={cn("font-serif text-xl font-semibold text-ink", className)}
      {...props}
    />
  );
}

function Description({ className, ...props }: ComponentProps<typeof VaulDrawer.Description>) {
  return (
    <VaulDrawer.Description
      data-slot="drawer-description"
      className={cn("font-sans text-base text-ink-soft", className)}
      {...props}
    />
  );
}

export const Drawer = {
  Root: VaulDrawer.Root,
  Trigger: VaulDrawer.Trigger,
  Close: VaulDrawer.Close,
  Content,
  Title,
  Description,
};
