import Link from "next/link";
import type { ReactNode } from "react";
import { brand } from "../../brand";

/** Signed-out frame: the wordmark, then the page. No navigation until there is somewhere to go. */
export default function AuthLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-dvh flex-col bg-paper">
      <div className="px-(--space-gutter) pt-6 lg:absolute lg:inset-x-0 lg:top-0">
        <Link
          href="/"
          className="rounded-field font-serif text-xl font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          {brand.wordmark}
        </Link>
      </div>
      <main className="flex flex-1 flex-col">{children}</main>
    </div>
  );
}
