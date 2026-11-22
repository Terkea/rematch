"use client";

import { type ReactNode, useLayoutEffect, useRef } from "react";

// A primitive in each of its states, side by side. Hover and focus-visible are forced from the
// component's own CSS (ForcedStatesStyle): `Forced` sets `data-force` on the element that owns the
// state, so the cell shows exactly what a pointer or a keyboard would.

export const STATES = [
  { key: "rest", label: "Rest" },
  { key: "hover", label: "Hover" },
  { key: "focus", label: "Focus-visible" },
  { key: "disabled", label: "Disabled" },
  { key: "error", label: "Error" },
] as const;

export type StateKey = (typeof STATES)[number]["key"];

/**
 * Forces `state` on the first element inside that matches `target` (by default the first
 * focusable one), for as long as this is mounted.
 */
export function Forced({
  state,
  target = "button, input, [role='radio'], [role='switch'], [role='combobox'], a[href]",
  children,
}: {
  state: "hover" | "focus-visible";
  target?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const el = ref.current?.querySelector(target);
    el?.setAttribute("data-force", state);
    return () => el?.removeAttribute("data-force");
  }, [state, target]);
  return (
    <span ref={ref} className="contents" data-forced={state}>
      {children}
    </span>
  );
}

/**
 * The reason a state has no look of its own, set where the specimen would be. Without a reason it
 * shows a dash, and the family's note gives the reason once.
 */
export function NotUsed({ children }: { children?: ReactNode }) {
  if (!children)
    return (
      <span className="block text-sm text-ink-faint">
        <span aria-hidden>–</span>
        <span className="sr-only">None</span>
      </span>
    );
  return <span className="block max-w-[24ch] text-sm text-ink-faint">{children}</span>;
}

export interface StateRow {
  name: string;
  render: (state: StateKey) => ReactNode;
}

/**
 * From 1024px one row per variant with the states as columns, named once above (for sight only:
 * every cell also names its state, visibly below 1024px and to assistive tech above it). Below
 * 1024px each variant is its own block.
 */
export function StateGrid({ rows }: { rows: StateRow[] }) {
  return (
    <div className="flex flex-col">
      <div
        aria-hidden
        className="hidden px-3 lg:grid lg:grid-cols-[--spacing(32)_repeat(5,minmax(0,1fr))] lg:gap-x-4"
      >
        <span />
        {STATES.map((s) => (
          <span key={s.key} className="pb-2 text-sm font-medium text-ink-soft">
            {s.label}
          </span>
        ))}
      </div>
      {rows.map((row) => (
        <div
          key={row.name}
          data-state-row={row.name}
          className="grid grid-cols-1 gap-x-4 gap-y-3 rounded-field px-3 py-4 odd:bg-paper sm:grid-cols-2 sm:gap-y-4 lg:grid-cols-[--spacing(32)_repeat(5,minmax(0,1fr))] lg:items-center lg:gap-y-0"
        >
          <h4 className="col-span-full font-medium lg:col-span-1">{row.name}</h4>
          {STATES.map((s) => (
            <div
              key={s.key}
              data-state={s.key}
              className="grid min-w-0 grid-cols-[--spacing(26)_minmax(0,1fr)] items-center justify-items-start gap-3 sm:flex sm:flex-col sm:items-start sm:gap-1.5 lg:block"
            >
              <span className="text-sm text-ink-faint lg:sr-only">{s.label}</span>
              {row.render(s.key)}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
