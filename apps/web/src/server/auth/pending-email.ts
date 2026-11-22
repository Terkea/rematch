// The address a sign-in link went to, carried to /sign-in/check-email without ever being in a URL
// (Task 21 review I1: URLs end up in server and proxy logs, history and sync). A successful
// `POST /sign-in/magic-link` sets a short HttpOnly cookie scoped to the check-email page; the
// proxy (`src/proxy.ts`) hands it to that page once, as a request header, and clears it.
// The same cookie is set for invited and uninvited addresses, so it reveals nothing (ADR-WEB-3).

export const PENDING_EMAIL_COOKIE = "ci_pending_email";
/** The request header the proxy sets for the page; any copy sent by the browser is dropped. */
export const PENDING_EMAIL_HEADER = "x-ci-pending-email";
export const PENDING_EMAIL_PATH = "/sign-in/check-email";
/** As long as the link itself lives (`magicLink({ expiresIn: 600 })`). */
export const PENDING_EMAIL_MAX_AGE = 600;

const ADDRESS = /^[^\s@<>"'`()[\]\\,;:]+@[^\s@<>"'`()[\]\\,;:]+\.[^\s@<>"'`()[\]\\,;:]+$/;

/** The value as an address, or null for anything that isn't one (never rendered otherwise). */
export function asPendingEmail(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const email = value.trim().toLowerCase();
  return email.length <= 254 && ADDRESS.test(email) ? email : null;
}

/** Decodes the cookie value; null when it is missing, malformed or not an address. */
export function pendingEmailFrom(cookieValue: string | undefined): string | null {
  if (!cookieValue) return null;
  try {
    return asPendingEmail(decodeURIComponent(cookieValue));
  } catch {
    return null;
  }
}

/**
 * Adds the cookie to a successful magic-link send. `req` must be unread (its body is read from a
 * clone). Every other request and response passes through unchanged.
 */
export async function withPendingEmail(
  req: Request,
  res: Response,
  secure: boolean,
): Promise<Response> {
  if (req.method !== "POST" || !new URL(req.url).pathname.endsWith("/sign-in/magic-link")) {
    return res;
  }
  if (res.status !== 200) return res;
  const body = (await req
    .clone()
    .json()
    .catch(() => null)) as { email?: unknown } | null;
  const email = asPendingEmail(body?.email);
  if (!email) return res;
  const headers = new Headers(res.headers);
  headers.append(
    "set-cookie",
    `${PENDING_EMAIL_COOKIE}=${encodeURIComponent(email)}; Max-Age=${PENDING_EMAIL_MAX_AGE}; Path=${PENDING_EMAIL_PATH}; HttpOnly; SameSite=Lax${secure ? "; Secure" : ""}`,
  );
  return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
}
