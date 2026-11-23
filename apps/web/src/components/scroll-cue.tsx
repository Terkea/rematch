"use client";

import { type ReactNode, useEffect, useRef } from "react";

// A horizontal track that doesn't fit (the area tabs, the scope switch and the filter bar's chip
// rows on a phone): on load
// the current item is centred in view, and each edge with more to scroll fades out (a mask, so
// it works on any surface in both themes). The scroller is the wrapper's `[data-scroller]`, or
// the nearest scrolling element inside it (the segmented control's own track).

const FADE = "1.5rem";

/** The mask for the edges that have more: `start`, `end`, both or neither. */
function maskFor(start: boolean, end: boolean): string {
  if (!start && !end) return "";
  const from = start ? `transparent, black ${FADE}` : "black";
  const to = end ? `black calc(100% - ${FADE}), transparent` : "black";
  return `linear-gradient(to right, ${from}, ${to})`;
}

function findScroller(root: HTMLElement): HTMLElement | null {
  if (root.hasAttribute("data-scroller")) return root;
  const marked = root.querySelector<HTMLElement>("[data-scroller]");
  if (marked) return marked;
  for (const el of root.querySelectorAll<HTMLElement>("*")) {
    const x = getComputedStyle(el).overflowX;
    if (x === "auto" || x === "scroll") return el;
  }
  return null;
}

/** Marks the scroller's edges (`data-fade`) and masks the ones with more to see. */
function markEdges(el: HTMLElement): void {
  const max = el.scrollWidth - el.clientWidth;
  const start = max > 1 && el.scrollLeft > 1;
  const end = max > 1 && el.scrollLeft < max - 1;
  const fade = [start ? "start" : "", end ? "end" : ""].filter(Boolean).join(" ");
  if (fade) el.dataset.fade = fade;
  else delete el.dataset.fade;
  const mask = maskFor(start, end);
  el.style.maskImage = mask;
  el.style.setProperty("-webkit-mask-image", mask);
}

/** Centres `current` in `el` (clamped), without moving the page vertically. */
function reveal(el: HTMLElement, current: HTMLElement | null): void {
  const max = el.scrollWidth - el.clientWidth;
  if (!current || max <= 0) return;
  const box = el.getBoundingClientRect();
  const item = current.getBoundingClientRect();
  const left = item.left - box.left + el.scrollLeft - (el.clientWidth - item.width) / 2;
  el.scrollLeft = Math.max(0, Math.min(max, left));
}

export function ScrollCue({
  children,
  current = '[aria-current="page"], [data-checked]',
  className,
}: {
  children: ReactNode;
  /** A selector for the item to bring into view on load. */
  current?: string;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    const el = findScroller(root);
    if (!el) return;
    reveal(el, el.querySelector<HTMLElement>(current));
    markEdges(el);
    const update = () => markEdges(el);
    el.addEventListener("scroll", update, { passive: true });
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    ro?.observe(el);
    return () => {
      el.removeEventListener("scroll", update);
      ro?.disconnect();
    };
  }, [current]);
  return (
    <div ref={ref} data-scroll-cue="" className={className}>
      {children}
    </div>
  );
}
