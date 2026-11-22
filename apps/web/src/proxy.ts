import { type NextRequest, NextResponse } from "next/server";
import {
  PENDING_EMAIL_COOKIE,
  PENDING_EMAIL_HEADER,
  PENDING_EMAIL_PATH,
  pendingEmailFrom,
} from "./server/auth/pending-email";

/**
 * Hands /sign-in/check-email the address its link went to, once: the short-lived cookie set by
 * the magic-link send becomes a request header for the page and is cleared in the same response.
 * A copy of that header sent by the browser is always dropped. (Task 21 review I1.)
 */
export function proxy(request: NextRequest): NextResponse {
  const headers = new Headers(request.headers);
  headers.delete(PENDING_EMAIL_HEADER);
  const email = pendingEmailFrom(request.cookies.get(PENDING_EMAIL_COOKIE)?.value);
  if (email) headers.set(PENDING_EMAIL_HEADER, email);
  const res = NextResponse.next({ request: { headers } });
  if (request.cookies.has(PENDING_EMAIL_COOKIE)) {
    res.cookies.set(PENDING_EMAIL_COOKIE, "", {
      path: PENDING_EMAIL_PATH,
      maxAge: 0,
      httpOnly: true,
      sameSite: "lax",
    });
  }
  return res;
}

export const config = { matcher: ["/sign-in/check-email"] };
