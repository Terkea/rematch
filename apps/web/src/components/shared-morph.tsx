"use client";

import { animate, dur, ease, spring, useReducedMotion } from "@ci/ui/motion";
import { useRouter } from "next/navigation";
import { type ReactNode, useLayoutEffect, useRef } from "react";

// Shared-element morphs (DESIGN.md, choreography 4): a list card becomes the next page's header.
// The View Transitions API runs it where the browser has it; elsewhere the header plays a FLIP on
// the `page` spring from where the card was. Either way the morph replaces the page turn (the
// app template asks `morphPending()`), any input skips to the end (M7), and under reduced motion
// the link is a plain client navigation (M10).

type Pending = {
  name: string;
  at: number;
  mode: "vt" | "flip";
  /** Where the card was, in viewport coordinates (FLIP only). */
  rect: DOMRect | null;
  /** Lets the view transition capture the new page once its header has mounted. */
  arrived: () => void;
};

let pending: Pending | null = null;
/** A morph older than this is stale (its navigation failed or was abandoned). */
const FRESH_MS = 2000;
/** The longest the view transition waits for the new page before it captures what is there. */
const WAIT_MS = 1200;

/** True while a shared-element morph is taking the current navigation (no page turn then). */
export function morphPending(): boolean {
  return pending !== null && Date.now() - pending.at < FRESH_MS;
}

type ViewTransition = {
  ready: Promise<void>;
  finished: Promise<void>;
  skipTransition: () => void;
};
type VtDocument = Document & {
  startViewTransition?: (update: () => Promise<void>) => ViewTransition;
};

const ms = (s: number) => Math.round(s * 1000);
const bezier = (b: readonly number[]) => `cubic-bezier(${b.join(", ")})`;

/** Retimes the browser's own view-transition animations to the choreography's values. */
function retime(name: string) {
  const timing: Record<string, OptionalEffectTiming> = {
    [`::view-transition-group(${name})`]: { duration: ms(dur.draw), easing: bezier(ease.turn) },
    [`::view-transition-old(${name})`]: { duration: ms(dur.draw), easing: bezier(ease.turn) },
    [`::view-transition-new(${name})`]: { duration: ms(dur.draw), easing: bezier(ease.turn) },
    "::view-transition-old(root)": { duration: ms(dur.quick), easing: "linear" },
    // The page content arrives 120ms after the header lands, with opacity only.
    "::view-transition-new(root)": {
      delay: ms(dur.draw + 0.12),
      duration: ms(dur.quick),
      easing: "linear",
      fill: "backwards",
    },
  };
  for (const a of document.getAnimations()) {
    const effect = a.effect as (KeyframeEffect & { pseudoElement?: string | null }) | null;
    const t = effect?.pseudoElement ? timing[effect.pseudoElement] : undefined;
    if (effect && t) effect.updateTiming(t);
  }
}

/**
 * Wraps a card whose link opens a page with a `MorphArrival` of the same `name`. A plain left
 * click on any same-origin link inside is taken over: the card morphs into the header.
 */
export function MorphLink({
  name,
  children,
  className,
}: {
  name: string;
  children: ReactNode;
  className?: string;
}) {
  const router = useRouter();
  const reduced = useReducedMotion();
  const ref = useRef<HTMLDivElement>(null);

  const onClickCapture = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey)
      return;
    const link = (e.target as Element).closest?.("a[href]");
    const href = link?.getAttribute("href");
    const el = ref.current;
    if (!link || !el?.contains(link) || !href?.startsWith("/") || href.startsWith("//")) return;
    e.preventDefault();
    if (reduced) {
      router.push(href);
      return;
    }
    const doc = document as VtDocument;
    if (typeof doc.startViewTransition !== "function") {
      pending = {
        name,
        at: Date.now(),
        mode: "flip",
        rect: el.getBoundingClientRect(),
        arrived: () => {},
      };
      router.push(href);
      return;
    }
    let arrived = () => {};
    const landed = new Promise<void>((resolve) => {
      arrived = resolve;
    });
    pending = { name, at: Date.now(), mode: "vt", rect: null, arrived: () => arrived() };
    el.style.setProperty("view-transition-name", name);
    const t = doc.startViewTransition(async () => {
      // One element per name: the card gives it up as the new page's header takes it.
      el.style.removeProperty("view-transition-name");
      router.push(href);
      await Promise.race([landed, new Promise((r) => setTimeout(r, WAIT_MS))]);
    });
    const skip = () => t.skipTransition();
    const opts = { capture: true, once: true } as const;
    document.addEventListener("pointerdown", skip, opts);
    document.addEventListener("keydown", skip, opts);
    t.ready.then(() => retime(name)).catch(() => {});
    t.finished
      .catch(() => {})
      .finally(() => {
        document.removeEventListener("pointerdown", skip, opts);
        document.removeEventListener("keydown", skip, opts);
        el.style.removeProperty("view-transition-name");
        pending = null;
      });
  };

  return (
    <div ref={ref} onClickCapture={onClickCapture} className={className}>
      {children}
    </div>
  );
}

/**
 * The page side of a morph: `header` is the shared element, `children` the rest of the page,
 * which arrives with opacity only once the header has landed.
 */
export function MorphArrival({
  name,
  header,
  children,
  className,
}: {
  name: string;
  header: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  const reduced = useReducedMotion();
  const head = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: runs once, on arrival
  useLayoutEffect(() => {
    const p = pending;
    if (!p || p.name !== name || Date.now() - p.at > FRESH_MS) return;
    if (p.mode === "vt") {
      p.arrived();
      return;
    }
    pending = null;
    const h = head.current;
    if (!p.rect || !h || reduced) return;
    const r = h.getBoundingClientRect();
    const x = p.rect.left - r.left;
    const y = p.rect.top - r.top;
    const header = animate(h, { x: [x, 0], y: [y, 0] }, spring.page);
    const content = body.current
      ? animate(
          body.current,
          { opacity: [0, 1] },
          { delay: dur.draw + 0.12, duration: dur.quick, ease: "linear" },
        )
      : null;
    const finish = () => {
      header.complete();
      content?.complete();
    };
    const opts = { capture: true, once: true } as const;
    document.addEventListener("pointerdown", finish, opts);
    document.addEventListener("keydown", finish, opts);
    return () => {
      document.removeEventListener("pointerdown", finish, opts);
      document.removeEventListener("keydown", finish, opts);
    };
  }, []);

  return (
    <div className={className}>
      <div ref={head} style={{ viewTransitionName: name }}>
        {header}
      </div>
      <div ref={body} className="flex flex-col gap-10">
        {children}
      </div>
    </div>
  );
}
