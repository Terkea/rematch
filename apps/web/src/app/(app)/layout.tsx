import type { ReactNode } from "react";
import { requireUser } from "../../server/auth/session";
import { Header } from "./shell/header";

export const dynamic = "force-dynamic";

/**
 * The signed-in shell: the header, then one sheet per page (`template.tsx` turns it on route
 * changes). Every page below calls `requireUser()` too, so none relies on the layout alone.
 */
export default async function AppLayout({ children }: { children: ReactNode }) {
  const user = await requireUser();
  return (
    <div className="flex min-h-dvh flex-col bg-paper">
      <a
        href="#main"
        className="sr-only z-50 rounded-field bg-ink px-4 py-2 text-sm font-medium text-on-ink focus:not-sr-only focus:fixed focus:top-3 focus:left-3"
      >
        Skip to content
      </a>
      <Header email={user.email} />
      <main
        id="main"
        className="mx-auto flex w-full max-w-6xl flex-1 flex-col px-(--space-gutter) pt-6 pb-16 md:pt-10"
      >
        {children}
      </main>
    </div>
  );
}
