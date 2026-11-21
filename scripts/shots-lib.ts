export type ShotWidth = 390 | 1440;
export type ShotTheme = "light" | "dark";

export interface ShotsArgs {
  routes: string[];
  baseUrl: string;
  out: string;
  storage?: string;
  motion: boolean;
}

export const defaultRoutes = ["/styleguide", "/sign-in", "/lab"];
export const widths: readonly ShotWidth[] = [390, 1440];
export const themes: readonly ShotTheme[] = ["light", "dark"];

export function parseShotsArgs(argv: string[]): ShotsArgs {
  const args: ShotsArgs = {
    routes: [...defaultRoutes],
    baseUrl: process.env.E2E_BASE_URL ?? "http://localhost:3100",
    out: ".shots",
    motion: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = (): string => {
      const v = argv[++i];
      if (v === undefined) throw new Error(`${flag} needs a value`);
      return v;
    };
    if (flag === "--routes") {
      args.routes = value()
        .split(",")
        .map((r) => normaliseRoute(r.trim()))
        .filter(Boolean);
    } else if (flag === "--base-url") args.baseUrl = value();
    else if (flag === "--out") args.out = value();
    else if (flag === "--storage") args.storage = value();
    else if (flag === "--motion") args.motion = true;
    else throw new Error(`Unknown argument: ${flag}`);
  }
  return args;
}

// Git Bash on Windows rewrites a leading "/x" into "C:/Program Files/Git/x" before we see it.
const drivePath = /^[A-Za-z]:[\\/]/;

export function normaliseRoute(route: string): string {
  if (!route) return route;
  if (drivePath.test(route)) {
    console.warn(
      `Route "${route}" looks like a Windows path; Git Bash rewrote it. Set MSYS_NO_PATHCONV=1 or omit the leading slash.`,
    );
    return route;
  }
  return route.startsWith("/") ? route : `/${route}`;
}

function slug(route: string): string {
  const [path = "", hash] = route.split("#");
  const safe = (text: string) => text.replace(/[?:*"<>|\\]/g, "_");
  const base = safe(path.replace(/^\/+|\/+$/g, "").replace(/\//g, "-")) || "home";
  return hash ? `${base}--${safe(hash)}` : base;
}

export function shotPath(out: string, route: string, width: ShotWidth, theme: ShotTheme): string {
  return `${out.replace(/[\\/]+$/, "")}/${slug(route)}/${width}-${theme}.png`;
}

export interface Shot {
  route: string;
  width: ShotWidth;
  theme: ShotTheme;
  path: string;
}

export function shotMatrix(out: string, routes: string[]): Shot[] {
  return routes.flatMap((route) =>
    widths.flatMap((width) =>
      themes.map((theme) => ({ route, width, theme, path: shotPath(out, route, width, theme) })),
    ),
  );
}
