// The app's sections, in header order. Each label matches its page's title (ruling: "Overview",
// not "Dashboard"; the old name stays a command-palette keyword).
export const navItems = [
  { label: "Overview", href: "/app", keywords: ["home", "dashboard"] },
  { label: "Games", href: "/app/games", keywords: ["reviews", "analysis"] },
  { label: "Accounts", href: "/app/accounts", keywords: ["chess.com", "lichess", "link"] },
] as const;

export type NavItem = (typeof navItems)[number];

/** `/app` is current only on itself; every other section also owns its nested routes. */
export function isCurrent(pathname: string, href: string): boolean {
  if (href === "/app") return pathname === "/app";
  return pathname === href || pathname.startsWith(`${href}/`);
}
