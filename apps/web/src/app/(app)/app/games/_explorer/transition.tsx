"use client";

import { useRouter } from "next/navigation";
import {
  createContext,
  type ReactNode,
  type TransitionStartFunction,
  useContext,
  useTransition,
} from "react";

type Nav = { pending: boolean; start: TransitionStartFunction };
const ExplorerNav = createContext<Nav | null>(null);

/**
 * One transition shared by the filter bar and the list, so a filter change keeps the old list in
 * place, dimmed, until the new page has rendered: never a spinner (B18).
 */
export function ExplorerTransition({ children }: { children: ReactNode }) {
  const [pending, start] = useTransition();
  return <ExplorerNav.Provider value={{ pending, start }}>{children}</ExplorerNav.Provider>;
}

/** `replace` to a new URL inside the shared transition (or a local one, outside the provider). */
export function useExplorerNavigation() {
  const shared = useContext(ExplorerNav);
  const [localPending, localStart] = useTransition();
  const router = useRouter();
  const start = shared?.start ?? localStart;
  return {
    pending: shared?.pending ?? localPending,
    start,
    replace: (href: string) => router.replace(href, { scroll: false }),
  };
}

/** The list while a filter change loads: 60% opacity and `aria-busy`, nothing moves. */
export function PendingRegion({ children }: { children: ReactNode }) {
  const { pending } = useExplorerNavigation();
  return (
    <div
      data-pending={pending ? "" : undefined}
      aria-busy={pending}
      className="flex flex-col gap-4 transition-opacity duration-(--dur-quick) ease-out data-pending:opacity-60 motion-reduce:transition-none"
    >
      {children}
    </div>
  );
}
