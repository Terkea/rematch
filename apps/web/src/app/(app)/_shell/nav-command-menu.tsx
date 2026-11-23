"use client";

import { type Area, areaLabels } from "@ci/ui/area-score";
import { CommandMenu, type CommandMenuItem } from "@ci/ui/overlays";
import { useRouter, useSearchParams } from "next/navigation";
import { useMemo } from "react";
import { coachChatLive } from "../../../lib/coach-chat";
import { shownNavItems, withFilters } from "./nav";
import { useThemeFlip } from "./theme-toggle";

export interface NavCommandMenuProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSignOut: () => void;
}

/** The seven skill areas, in `areaLabels` order (the same order the area tabs use). */
const areas = Object.entries(areaLabels) as [Area, string][];

/** Ctrl K: every section, the skill areas, asking the coach, the theme and signing out. */
export function NavCommandMenu({ open, onOpenChange, onSignOut }: NavCommandMenuProps) {
  const router = useRouter();
  // The page's filters travel with every page it opens (audit A7).
  const search = useSearchParams();
  const theme = useThemeFlip();
  const items = useMemo<CommandMenuItem[]>(
    () => [
      ...shownNavItems(coachChatLive()).map((item) => ({
        id: `go:${item.href}`,
        label: `Go to ${item.label.toLowerCase()}`,
        group: "Pages",
        keywords: [...item.keywords],
        onSelect: () => router.push(withFilters(item.href, search)),
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
      ...areas.map(([area, label]) => ({
        id: `go:/app/areas/${area}`,
        label,
        group: "Skill areas",
        keywords: ["area", "skill"],
        onSelect: () => router.push(withFilters(`/app/areas/${area}`, search)),
      })),
      ...(coachChatLive()
        ? [
            {
              id: "coach:new",
              label: "Ask your coach",
              group: "Coach",
              keywords: ["question", "chat", "ask"],
              onSelect: () => router.push(withFilters("/app/coach/new", search)),
            },
          ]
        : []),
      {
        id: "go:/licenses",
        label: "Go to licences",
        group: "Pages",
        keywords: ["credits", "attribution", "open source"],
        onSelect: () => router.push("/licenses"),
      },
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
    [router, search, theme.label, theme.flip, onSignOut],
  );
  return <CommandMenu items={items} open={open} onOpenChange={onOpenChange} />;
}
