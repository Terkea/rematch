"use client";

import { useTheme } from "next-themes";
import { useSyncExternalStore } from "react";

const options = [
  { value: "light", label: "Day" },
  { value: "dark", label: "Night" },
  { value: "system", label: "System" },
] as const;

const noop = () => () => {};

/** Day, night or system, as toggle buttons. Tone changes on hover, never position (M5). */
export function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  // The stored theme is only known on the client, so nothing is pressed during the server render.
  const mounted = useSyncExternalStore(
    noop,
    () => true,
    () => false,
  );
  return (
    // biome-ignore lint/a11y/useSemanticElements: a labelled group of toggle buttons, not a fieldset
    <div role="group" aria-label="Theme" className="flex rounded-pill bg-well p-0.5">
      {options.map((option) => {
        const pressed = mounted && theme === option.value;
        return (
          <button
            key={option.value}
            type="button"
            aria-pressed={pressed}
            onClick={() => setTheme(option.value)}
            className={
              pressed
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
