// The app's sections, in header order. Each label matches its page's title (ruling: "Overview",
// not "Dashboard"; the old name stays a command-palette keyword).
export const navItems = [
  { label: "Overview", href: "/app", keywords: ["home", "dashboard"] },
  { label: "Games", href: "/app/games", keywords: ["reviews", "analysis"] },
  { label: "Report", href: "/app/report", keywords: ["coaching", "plan", "summary"] },
  { label: "Weaknesses", href: "/app/weaknesses", keywords: ["mistakes", "leaks"] },
  { label: "Strengths", href: "/app/strengths", keywords: ["good", "keep doing", "best moves"] },
  { label: "Progress", href: "/app/progress", keywords: ["trends", "kpi", "weekly"] },
  { label: "Coach", href: "/app/coach", keywords: ["ask", "chat", "question"] },
  { label: "Accounts", href: "/app/accounts", keywords: ["chess.com", "lichess", "link"] },
] as const;

export type NavItem = (typeof navItems)[number];

/** The sections the header shows: Coach only once the chat pages are live (`coachChatLive`). */
export function shownNavItems(chatLive: boolean): readonly NavItem[] {
  return chatLive ? navItems : navItems.filter((i) => i.href !== "/app/coach");
}

/**
 * The sections the nav keeps below `lg` (eight don't fit until 1024px; at 640 to 1023px the row
 * scrolled sideways). The rest stay in the header from `lg` up and, below it, move into the
 * account menu ("More pages"), so the row never scrolls sideways.
 */
export const phoneNavHrefs: readonly string[] = ["/app", "/app/games", "/app/report", "/app/coach"];

export const onPhoneRow = (href: string): boolean => phoneNavHrefs.includes(href);

/** `/app` is current only on itself; every other section also owns its nested routes. */
export function isCurrent(pathname: string, href: string): boolean {
  if (href === "/app") return pathname === "/app";
  return pathname === href || pathname.startsWith(`${href}/`);
}
