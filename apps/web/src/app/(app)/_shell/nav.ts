// The app's sections, in header order. Each label matches its page's title (ruling: "Overview",
// not "Dashboard"; the old name stays a command-palette keyword).
export const navItems = [
  { label: "Overview", href: "/app", keywords: ["home", "dashboard"] },
  { label: "Games", href: "/app/games", keywords: ["reviews", "analysis"] },
  { label: "Report", href: "/app/report", keywords: ["coaching", "plan", "summary"] },
  { label: "Weaknesses", href: "/app/weaknesses", keywords: ["mistakes", "leaks"] },
  { label: "Progress", href: "/app/progress", keywords: ["trends", "kpi", "weekly"] },
  { label: "Coach", href: "/app/coach", keywords: ["ask", "chat", "question"] },
  { label: "Accounts", href: "/app/accounts", keywords: ["chess.com", "lichess", "link"] },
] as const;

export type NavItem = (typeof navItems)[number];

/**
 * The sections a phone's nav row keeps (seven don't fit at 360px). The rest stay in the header
 * from `sm` up and, on a phone, move into the account menu ("More pages"), so the row never
 * scrolls sideways.
 */
export const phoneNavHrefs: readonly string[] = ["/app", "/app/games", "/app/report", "/app/coach"];

export const onPhoneRow = (href: string): boolean => phoneNavHrefs.includes(href);

/** `/app` is current only on itself; every other section also owns its nested routes. */
export function isCurrent(pathname: string, href: string): boolean {
  if (href === "/app") return pathname === "/app";
  return pathname === href || pathname.startsWith(`${href}/`);
}
