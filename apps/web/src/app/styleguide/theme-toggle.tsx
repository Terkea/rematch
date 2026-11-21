"use client";

import { useTheme } from "next-themes";
import { useSyncExternalStore } from "react";

const options = [
  { value: "light", label: "Day" },
  { value: "dark", label: "Night" },
  { value: "system", label: "System" },
] as const;

const noop = () => () => {};

/** Day, night or system. A segmented control: tone changes on hover, never position (M5). */
export function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  // The stored theme is only known on the client, so nothing is checked during the server render.
  const mounted = useSyncExternalStore(
    noop,
    () => true,
    () => false,
  );
  return (
    <div role="radiogroup" aria-label="Theme" className="flex rounded-pill bg-well p-0.5">
      {options.map((option) => {
        const checked = mounted && theme === option.value;
        return (
          // biome-ignore lint/a11y/useSemanticElements: a segmented control, styled as one group
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            onClick={() => setTheme(option.value)}
            className={
              checked
                ? "rounded-pill bg-ink px-3 py-1 text-sm font-medium text-on-ink"
                : "rounded-pill px-3 py-1 text-sm font-medium text-ink-soft hover:bg-sheet"
            }
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
