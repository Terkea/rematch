"use client";

import { Icon, MagnifyingGlassIcon } from "@ci/ui/icons";
import { motion, spring, useReducedMotion } from "@ci/ui/motion";
import { Button, Kbd } from "@ci/ui/primitives";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState } from "react";
import { brand } from "../../../brand";
import { isCurrent, navItems } from "./nav";
import { NavCommandMenu } from "./nav-command-menu";
import { ThemeToggle } from "./theme-toggle";
import { UserMenu, useSignOut } from "./user-menu";

export interface HeaderProps {
  email: string;
  signOutTo?: (href: string) => void;
}

const navLink = [
  "relative flex h-10 items-center rounded-field px-3 text-sm font-medium",
  "transition-colors duration-(--dur-quick) ease-out",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
].join(" ");

/**
 * The app header. It stays still on route changes (only the sheet turns, choreography 5). The ink
 * rule under the current section slides to the next one on `page`, showing where you went (M1).
 * Below `md` the sections drop to their own row. `data-slot="activity"` is Task 27's import
 * indicator.
 */
export function Header({ email, signOutTo = (href) => window.location.assign(href) }: HeaderProps) {
  const pathname = usePathname();
  const reduced = useReducedMotion();
  const [menuOpen, setMenuOpen] = useState(false);
  const signOut = useSignOut(signOutTo);

  return (
    <header className="sticky top-0 z-40 bg-paper">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-x-6 px-(--space-gutter) pt-3 md:h-16 md:flex-nowrap md:pt-0">
        <Link
          href="/"
          className="flex h-10 items-center rounded-field font-serif text-xl font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          {brand.wordmark}
        </Link>

        <nav
          aria-label="Main"
          className="order-last -mx-3 basis-full md:order-none md:mx-0 md:basis-auto"
        >
          <ul className="flex items-center gap-1">
            {navItems.map((item) => {
              const current = isCurrent(pathname, item.href);
              return (
                <li key={item.href}>
                  <Link
                    href={item.href}
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

        <div className="ml-auto flex items-center gap-2">
          <div data-slot="activity" className="flex items-center empty:hidden" />
          <Button
            variant="quiet"
            size="sm"
            onClick={() => setMenuOpen(true)}
            className="max-sm:w-8 max-sm:px-0"
          >
            <Icon icon={MagnifyingGlassIcon} />
            <span className="max-sm:sr-only">Search</span>
            <Kbd className="max-sm:hidden">Ctrl K</Kbd>
          </Button>
          <ThemeToggle />
          <UserMenu email={email} signOutTo={signOutTo} />
        </div>
      </div>
      <NavCommandMenu open={menuOpen} onOpenChange={setMenuOpen} onSignOut={signOut.run} />
    </header>
  );
}
