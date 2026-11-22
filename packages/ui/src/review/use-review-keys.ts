"use client";

import { useEffect, useRef } from "react";

export type ReviewKeyAction =
  | "prev"
  | "next"
  | "first"
  | "last"
  | "prevMoment"
  | "nextMoment"
  | "flip"
  | "play"
  | "engine";

/** The review's key map, by `KeyboardEvent.key`. The controls' tooltips show the same keys. */
export const REVIEW_KEYS: Readonly<Record<string, ReviewKeyAction>> = {
  ArrowLeft: "prev",
  ArrowRight: "next",
  Home: "first",
  End: "last",
  ArrowUp: "prevMoment",
  ArrowDown: "nextMoment",
  f: "flip",
  F: "flip",
  " ": "play",
  b: "engine",
  B: "engine",
};

/**
 * Targets that own these keys themselves. The brief's list, plus the Board's `SquareGrid` (a
 * `role="group"` of square buttons whose arrows move focus) and the other composite widgets whose
 * arrows move inside them (tabs, menus, listboxes, radio groups, comboboxes).
 */
export const REVIEW_KEY_GUARD = [
  "input",
  "textarea",
  "select",
  "[contenteditable]:not([contenteditable='false'])",
  '[role="grid"]',
  '[role="slider"]',
  '[role="dialog"]',
  "[data-square-button]",
  '[role="textbox"]',
  '[role="spinbutton"]',
  '[role="combobox"]',
  '[role="listbox"]',
  '[role="menu"]',
  '[role="tablist"]',
  '[role="radiogroup"]',
].join(",");

/** Space activates a focused button, link or toggle natively; the review leaves it alone. */
const ACTIVATES_ON_SPACE =
  'button,a[href],summary,[role="button"],[role="link"],[role="switch"],[role="checkbox"],[role="tab"],[role="menuitem"],[role="option"],[role="radio"]';

/** Actions that toggle state: a held key fires them once, not on every repeat. */
const TOGGLES: ReadonlySet<ReviewKeyAction> = new Set(["play", "flip", "engine"]);

export function reviewKeyAction(event: KeyboardEvent): ReviewKeyAction | null {
  if (event.defaultPrevented || event.isComposing) return null;
  if (event.ctrlKey || event.altKey || event.metaKey) return null;
  const action = REVIEW_KEYS[event.key];
  if (!action) return null;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest(REVIEW_KEY_GUARD)) return null;
  if (action === "play" && target?.closest(ACTIVATES_ON_SPACE)) return null;
  return action;
}

/**
 * Binds the review keys on `window`: ArrowLeft and ArrowRight step, Home and End jump, ArrowUp and
 * ArrowDown go to the previous or next mistake, `f` flips, Space plays (its scroll prevented), and
 * `b` shows the engine move. It ignores keys with Ctrl, Alt or Meta, keys a focused widget already
 * handled, and keys from inside REVIEW_KEY_GUARD. The map is read on each key, so a new map never
 * rebinds the listener.
 */
export function useReviewKeys(
  map: Partial<Record<ReviewKeyAction, () => void>>,
  opts: { enabled?: boolean } = {},
): void {
  const enabled = opts.enabled ?? true;
  const latest = useRef(map);
  latest.current = map;

  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const action = reviewKeyAction(event);
      const run = action ? latest.current[action] : undefined;
      if (!run || !action) return;
      event.preventDefault();
      // A held toggle fires once; its repeats are still kept from scrolling the page.
      if (event.repeat && TOGGLES.has(action)) return;
      run();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [enabled]);
}
