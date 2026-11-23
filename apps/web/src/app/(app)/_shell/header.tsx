"use client";

import { Icon, MagnifyingGlassIcon } from "@ci/ui/icons";
import { motion, spring, useReducedMotion } from "@ci/ui/motion";
import { Button, Kbd } from "@ci/ui/primitives";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { type ReactNode, useState } from "react";
import { brand } from "../../../brand";
import { coachChatLive } from "../../../lib/coach-chat";
import { isCurrent, onPhoneRow, shownNavItems, withFilters } from "./nav";
import { NavCommandMenu } from "./nav-command-menu";
import { ThemeToggle } from "./theme-toggle";
import { UserMenu, useSignOut } from "./user-menu";

export interface HeaderProps {
  email: string;
  /** The user's name; for a Lichess-created account, the Lichess username. */
  name?: string;
  signOutTo?: (href: string) => void;
  /** Background work, shown in `data-slot="activity"` (the layout passes `ActivityIndicator`). */
  activity?: ReactNode;
}

const navLink = [
  "relative flex h-10 items-center rounded-field px-3 text-sm font-medium",
  "transition-colors duration-(--dur-quick) ease-out",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
].join(" ");

/**
 * The app header. It stays still on route changes (only the sheet turns, choreography 5). The ink
 * rule under the current section slides to the next one on `page`, showing where you went (M1).
 * Below `md` the sections drop to their own row, and below `lg` only the phone-row ones stay (the
 * nine do not fit until 1024px); the rest move into the account menu's "More pages". `data-slot="activity"` holds `activity`, the
 * import indicator; it collapses while that renders nothing.
 */
export function Header({
  email,
  name = "",
  signOutTo = (href) => window.location.assign(href),
  activity,
}: HeaderProps) {
  const pathname = usePathname();
  // The page's filters travel with every section link (audit A7).
  const search = useSearchParams();
  const reduced = useReducedMotion();
  const [menuOpen, setMenuOpen] = useState(false);
  const signOut = useSignOut(signOutTo);
  const sections = shownNavItems(coachChatLive());

  return (
    <header data-app-chrome="" className="sticky top-0 z-40 bg-paper">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-x-4 px-(--space-gutter) md:gap-x-6 pt-3 md:h-16 md:flex-nowrap md:pt-0">
        <Link
          href="/"
          className="flex h-10 shrink-0 items-center rounded-field font-serif text-xl font-semibold whitespace-nowrap text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          {brand.wordmark}
        </Link>

        <nav
          aria-label="Main"
          className="order-last shrink-0 -mx-3 basis-full md:order-none md:mx-0 md:basis-auto"
        >
          <ul className="flex items-center gap-1">
            {sections.map((item) => {
              const current = isCurrent(pathname, item.href);
              return (
                <li key={item.href} className={onPhoneRow(item.href) ? undefined : "max-lg:hidden"}>
                  <Link
                    href={withFilters(item.href, search)}
                    aria-current={current ? "page" : undefined}
                    className={`${navLink} ${current ? "font-semibold text-ink" : "text-ink-soft hover:bg-sheet hover:text-ink"}`}
                  >
                    {item.label}
                    {current ? (
                      <motion.span
                        aria-hidden="true"
                        layoutId="nav-current"
                        className="absolute inset-x-3 bottom-0 h-0.5 rounded-pill bg-ink"
                        transition={reduced ? { duration: 0 } : spring.page}
                      />
                    ) : null}
                  </Link>
                </li>
              );
            })}
          </ul>
        </nav>

        {/* A zero basis: on a phone this group never wraps under the wordmark, it takes the rest
            of the row; only the activity line gives way (I7). */}
        <div className="group/right flex min-w-0 flex-1 basis-0 items-center justify-end gap-2">
          <div data-slot="activity" className="flex min-w-0 items-center empty:hidden">
            {activity}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              variant="quiet"
              size="sm"
              onClick={() => setMenuOpen(true)}
              className="max-sm:w-8 max-sm:px-0"
            >
              <Icon icon={MagnifyingGlassIcon} />
              {/* Beside the one-row nav the icon and Ctrl K say it, and while background work shows
                there the icon alone; the word stays for AT. */}
              <span className="max-sm:sr-only md:sr-only">Search</span>
              <Kbd className="max-sm:hidden md:group-has-[[data-slot=activity-indicator]]/right:hidden">
                Ctrl K
              </Kbd>
            </Button>
            <ThemeToggle />
            <UserMenu
              email={email}
              name={name}
              signOutTo={signOutTo}
              morePages={sections
                .filter((item) => !onPhoneRow(item.href))
                .map((item) => ({ ...item, link: withFilters(item.href, search) }))}
              pathname={pathname}
            />
          </div>
        </div>
      </div>
      <NavCommandMenu open={menuOpen} onOpenChange={setMenuOpen} onSignOut={signOut.run} />
    </header>
  );
}
