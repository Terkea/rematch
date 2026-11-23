// The app's sections, in header order. Each label matches its page's title (ruling: "Overview",
// not "Dashboard"; the old name stays a command-palette keyword).
export const navItems = [
  { label: "Overview", href: "/app", keywords: ["home", "dashboard"] },
  { label: "Games", href: "/app/games", keywords: ["reviews", "analysis"] },
  { label: "Report", href: "/app/report", keywords: ["coaching", "plan", "summary"] },
  { label: "Weaknesses", href: "/app/weaknesses", keywords: ["mistakes", "leaks"] },
  {
    label: "Train",
    href: "/app/train",
    keywords: ["practise", "drill", "puzzles", "missed moments"],
  },
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
 * The sections the nav keeps below `lg` (nine don't fit until 1024px; at 640 to 1023px the row
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

const DAY = /^\d{4}-\d{2}-\d{2}$/;
const TC = /^(bullet|blitz|rapid|classical|daily|all)$/;
const ID = /^[0-9A-Za-z-]{1,64}$/;

/** The filters of the current URL, each checked; a bad value is dropped (the page parses again). */
type Search = { get(name: string): string | null };

function carried(search: Search) {
  const get = (k: string, re: RegExp) => {
    const v = search.get(k);
    return v !== null && re.test(v) ? v : null;
  };
  const colour = search.get("color") ?? search.get("colour");
  return {
    account: get("account", ID),
    tc: get("tc", TC),
    color: colour === "white" || colour === "black" ? colour : null,
    from: get("from", DAY),
    to: get("to", DAY),
  };
}

/**
 * `href` with the filters of the current page (audit A7): the coach scope (`account`, `tc`), the
 * colour and the dates, so moving between pages keeps what the player chose. Each page takes
 * what its contract reads: the games page its own `colour` and no account, the coach pages only
 * the scope (a new chat also the colour), and the pages without filters nothing.
 */
export function withFilters(href: string, search: Search | null): string {
  if (!search) return href;
  const f = carried(search);
  const q = new URLSearchParams();
  const set = (k: string, v: string | null) => {
    if (v !== null) q.set(k, v);
  };
  if (href === "/app/games") {
    set("tc", f.tc);
    set("colour", f.color);
    set("from", f.from);
    set("to", f.to);
  } else if (href === "/app/train") {
    set("account", f.account);
    set("tc", f.tc === "all" ? null : f.tc);
  } else if (href === "/app/coach" || href.startsWith("/app/coach/")) {
    set("account", f.account);
    set("tc", f.tc === "all" ? null : f.tc);
    if (href === "/app/coach/new") set("color", f.color);
  } else if (
    href === "/app" ||
    /^\/app\/(report|weaknesses|strengths|progress|areas)(\/|$)/.test(href)
  ) {
    set("account", f.account);
    set("tc", f.tc);
    set("color", f.color);
    set("from", f.from);
    set("to", f.to);
  }
  const s = q.toString();
  return s ? `${href}?${s}` : href;
}
