/** The only origins a game's source link may point at, per platform (ADR 0049). */
const ALLOWED: Record<"lichess" | "chesscom", { header: string; host: string }> = {
  chesscom: { header: "Link", host: "www.chess.com" },
  lichess: { header: "Site", host: "lichess.org" },
};

/**
 * The game's page on its platform, read from the PGN headers: `Link` for chess.com, `Site` for
 * Lichess. Anything that is not a plain `https://` URL on the platform's own host (no other
 * scheme, port or credentials) gives null, so a crafted header can never become a link.
 */
export function sourceUrl(
  platform: "lichess" | "chesscom",
  headers: Record<string, string>,
): string | null {
  const rule = ALLOWED[platform];
  const raw = Object.hasOwn(headers, rule.header) ? headers[rule.header] : undefined;
  if (!raw) return null;
  let url: URL;
  try {
    url = new URL(raw.trim());
  } catch {
    return null;
  }
  if (url.protocol !== "https:" || url.hostname !== rule.host) return null;
  if (url.port !== "" || url.username !== "" || url.password !== "") return null;
  return url.href;
}
