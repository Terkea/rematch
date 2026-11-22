"use client";

import { Toaster } from "@ci/ui/overlays";
import { ThemeProvider } from "next-themes";
import type { ReactNode } from "react";
import { QueryProvider } from "../lib/realtime/query-provider";

/**
 * App-wide client providers: theme, TanStack Query (Task 21's account mutations, Task 23's
 * stream cache) and the one Toaster. The signed-in layout adds `RealtimeProvider` (Task 27).
 */
export function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider attribute="data-theme" defaultTheme="system" enableSystem enableColorScheme>
      <QueryProvider>
        {children}
        <Toaster />
      </QueryProvider>
    </ThemeProvider>
  );
}
