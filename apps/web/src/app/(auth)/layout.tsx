import Link from "next/link";
import type { ReactNode } from "react";
import { brand } from "../../brand";
import { BrandLockup } from "../../components/brand/lockup";
import { AUTH_COLUMN, AUTH_GRID } from "./auth-page";

/**
 * Signed-out frame: the wordmark, then the page. No navigation until there is somewhere to go. The
 * wordmark sits in the same grid cell and column as the form, so it shares the title's left edge.
 */
export default function AuthLayout({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-dvh flex-col bg-paper">
      <div className={`pt-6 lg:absolute lg:inset-x-0 lg:top-0 ${AUTH_GRID}`}>
        <div className="px-(--space-gutter)">
          <div className={AUTH_COLUMN}>
            <Link
          href="/"
          aria-label={brand.name}
          className="inline-flex h-10 items-center gap-2 rounded-field whitespace-nowrap text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
              <BrandLockup />
            </Link>
          </div>
        </div>
      </div>
      <main className="flex flex-1 flex-col">{children}</main>
    </div>
  );
}
