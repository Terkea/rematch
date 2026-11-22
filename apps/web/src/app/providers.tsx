"use client";

import { Toaster } from "@ci/ui/overlays";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ThemeProvider } from "next-themes";
import { type ReactNode, useState } from "react";

/**
 * App-wide client providers: theme, TanStack Query (Task 21's account mutations) and the one
 * Toaster. Task 27 adds the realtime provider.
 */
export function Providers({ children }: { children: ReactNode }) {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: 1 } } }),
  );
  return (
    <ThemeProvider attribute="data-theme" defaultTheme="system" enableSystem enableColorScheme>
      <QueryClientProvider client={client}>
        {children}
        <Toaster />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
