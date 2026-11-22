// The app's sections, in header order. Dashboard and Games are the next pages to land (the
// owner's analysed games); their routes already exist as quiet placeholders.
export const navItems = [
  { label: "Dashboard", href: "/app", keywords: ["home", "overview"] },
  { label: "Games", href: "/app/games", keywords: ["reviews", "analysis"] },
  { label: "Accounts", href: "/app/accounts", keywords: ["chess.com", "lichess", "link"] },
] as const;

export type NavItem = (typeof navItems)[number];

/** `/app` is current only on itself; every other section also owns its nested routes. */
export function isCurrent(pathname: string, href: string): boolean {
  if (href === "/app") return pathname === "/app";
  return pathname === href || pathname.startsWith(`${href}/`);
}
