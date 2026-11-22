"use client";

import { Icon, MoonIcon, SunIcon } from "@ci/ui/icons";
import { AnimatePresence, dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import { Button } from "@ci/ui/primitives";
import { useTheme } from "next-themes";
import { useSyncExternalStore } from "react";

const noop = () => () => {};

/** True after hydration: the stored theme is only known on the client. */
export function useMounted(): boolean {
  return useSyncExternalStore(
    noop,
    () => true,
    () => false,
  );
}

/** The current theme and a flip to the other one, for the toggle and the command menu. */
export function useThemeFlip() {
  const { resolvedTheme, setTheme } = useTheme();
  const dark = resolvedTheme === "dark";
  return {
    dark,
    label: dark ? "Use light theme" : "Use dark theme",
    flip: () => setTheme(dark ? "light" : "dark"),
  };
}

/**
 * One button that names what it will do. The glyph turns a quarter as it swaps (sun for moon),
 * so the change reads as the switch itself (M1); under reduced motion it only cross-fades.
 */
export function ThemeToggle() {
  const mounted = useMounted();
  const { dark, label, flip } = useThemeFlip();
  const reduced = useReducedMotion();
  return (
    <Button
      variant="quiet"
      size="sm"
      aria-label={mounted ? label : "Theme"}
      onClick={flip}
      className="w-8 px-0"
    >
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={mounted && dark ? "sun" : "moon"}
          className="flex"
          initial={reduced ? { opacity: 0 } : { opacity: 0, rotate: -90, scale: 0.8 }}
          animate={{ opacity: 1, rotate: 0, scale: 1 }}
          exit={reduced ? { opacity: 0 } : { opacity: 0, rotate: 90, scale: 0.8 }}
          transition={{ duration: dur.quick, ease: [...ease.out] }}
        >
          <Icon icon={mounted && dark ? SunIcon : MoonIcon} />
        </motion.span>
      </AnimatePresence>
    </Button>
  );
}
