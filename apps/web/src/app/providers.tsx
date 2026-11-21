"use client";

import { ThemeProvider } from "next-themes";
import type { ReactNode } from "react";

/** App-wide client providers. Task 27 adds the query and realtime providers. */
export function Providers({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider attribute="data-theme" defaultTheme="system" enableSystem enableColorScheme>
      {children}
    </ThemeProvider>
  );
}
