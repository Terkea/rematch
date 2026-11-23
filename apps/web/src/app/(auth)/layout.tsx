import Link from "next/link";
import type { ReactNode } from "react";
import { brand } from "../../brand";
import { BrandWordmark } from "../../components/brand/lockup";

/**
 * Signed-out frame: the wordmark, then the page. No navigation until there is somewhere to go.
 * The page itself carries the large mark (`AuthPage`), so the corner holds the word alone.
 */
export default function AuthLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-dvh flex-col bg-paper">
      <div className="px-(--space-gutter) pt-6 lg:absolute lg:inset-x-0 lg:top-0">
        <Link
          href="/"
          aria-label={brand.name}
          className="inline-flex h-10 items-center gap-2 rounded-field whitespace-nowrap text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          <BrandWordmark />
        </Link>
      </div>
      <main className="flex flex-1 flex-col">{children}</main>
    </div>
  );
}
