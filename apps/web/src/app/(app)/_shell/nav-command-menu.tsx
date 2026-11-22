"use client";

import { CommandMenu, type CommandMenuItem } from "@ci/ui/overlays";
import { useRouter } from "next/navigation";
import { useMemo } from "react";
import { navItems } from "./nav";
import { useThemeFlip } from "./theme-toggle";

export interface NavCommandMenuProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSignOut: () => void;
}

/** Ctrl K: every section, the theme and signing out. Task 27 adds more as pages land. */
export function NavCommandMenu({ open, onOpenChange, onSignOut }: NavCommandMenuProps) {
  const router = useRouter();
  const theme = useThemeFlip();
  const items = useMemo<CommandMenuItem[]>(
    () => [
      ...navItems.map((item) => ({
        id: `go:${item.href}`,
        label: `Go to ${item.label.toLowerCase()}`,
        group: "Pages",
        keywords: [...item.keywords],
        onSelect: () => router.push(item.href),
      })),
      ...(process.env.NODE_ENV !== "production"
        ? [
            {
              id: "go:/styleguide",
              label: "Go to styleguide",
              group: "Pages",
              keywords: ["design", "tokens"],
              onSelect: () => router.push("/styleguide"),
            },
          ]
        : []),
      {
        id: "theme",
        label: theme.label,
        group: "Preferences",
        keywords: ["dark", "light", "night", "day", "theme"],
        onSelect: theme.flip,
      },
      {
        id: "sign-out",
        label: "Sign out",
        group: "Account",
        keywords: ["log out"],
        onSelect: onSignOut,
      },
    ],
    [router, theme.label, theme.flip, onSignOut],
  );
  return <CommandMenu items={items} open={open} onOpenChange={onOpenChange} />;
}
