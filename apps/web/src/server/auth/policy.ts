import type { Db } from "@ci/db";
import type { BetterAuthOptions } from "better-auth";
import { sql } from "drizzle-orm";
import type { WebEnv } from "../../env";
import { magicLinkEmail, sendMail } from "../mail";
import { isInvited, NOT_INVITED, normaliseEmail } from "./invite";

/** Magic links an address may receive per window (on top of the per-IP limit). */
export const MAGIC_LINKS_PER_EMAIL = 3;
export const MAGIC_LINK_WINDOW_SECONDS = 600;

/** Error code for an OAuth identity whose email the provider has not verified. */
export const EMAIL_NOT_VERIFIED = "email_not_verified";

type ValidateUserInfo = NonNullable<NonNullable<BetterAuthOptions["user"]>["validateUserInfo"]>;

/**
 * Admission policy for every sign-in method (ADR-WEB-3):
 * - an OAuth identity may create or link an account only with a provider-verified email, so a
 *   provider account that merely claims an invited address cannot take that account over
 *   (Google is a trusted linking provider, which skips Better Auth's own verified check);
 * - with INVITE_ONLY on, only invited addresses may create an account.
 */
export function admissionPolicy(db: Db, env: WebEnv): ValidateUserInfo {
  return async ({ user, source }) => {
    const creates = source.action === "create-user";
    if (source.method === "oauth" && (creates || source.action === "link-account")) {
      if (user.emailVerified !== true) {
        return {
          error: EMAIL_NOT_VERIFIED,
          errorDescription: "The provider has not verified this email.",
        };
      }
    }
    if (!creates) return;
    if (await isInvited(db, env, String(user.email ?? ""))) return;
    return { error: NOT_INVITED, errorDescription: "This address cannot sign in yet." };
  };
}

/** How many magic links were issued for this address in the current window (including this one). */
async function recentMagicLinks(db: Db, email: string): Promise<number> {
  const rows = await db.execute<{ n: number }>(sql`
    select count(*)::int as n from verification
    where identifier like 'magic-link:%'
      and lower(normalize(trim(value::jsonb ->> 'email'), NFKC)) = ${email}
      and created_at > now() - make_interval(secs => ${MAGIC_LINK_WINDOW_SECONDS})`);
  return rows[0]?.n ?? 0;
}

/**
 * Sends the magic link if the address may have one. Runs after the response (see
 * `runAfterResponse`), so the request path is the same for every address: invited or not,
 * throttled or not, the endpoint answers `200 { status: true }` at the same point.
 */
export async function deliverMagicLink(
  db: Db,
  env: WebEnv,
  email: string,
  url: string,
): Promise<void> {
  const to = normaliseEmail(email);
  if (!to) return;
  if (!(await isInvited(db, env, to))) return;
  if ((await recentMagicLinks(db, to)) > MAGIC_LINKS_PER_EMAIL) return;
  await sendMail(magicLinkEmail(to, url));
}

/**
 * Client IP resolution for rate limits. Next route handlers do not expose the socket address, so
 * forwarding headers are trusted only from configured proxies (`TRUSTED_PROXIES`, CIDRs). Without
 * one, no header is read and every client shares one bucket per path (127.0.0.1 in dev and test);
 * the per-email throttle still applies.
 */
export function ipAddressOptions(env: WebEnv): {
  ipAddressHeaders: string[];
  trustedProxies?: string[];
} {
  if (env.TRUSTED_PROXIES.length === 0) return { ipAddressHeaders: [] };
  return { ipAddressHeaders: env.IP_ADDRESS_HEADERS, trustedProxies: env.TRUSTED_PROXIES };
}

const LOCAL_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);

/**
 * Secure cookies follow the base URL: https gives `__Secure-ci.session_token`, http gives
 * `ci.session_token`. In production the base URL must be https, except on a local host.
 */
export function secureCookies(env: WebEnv, nodeEnv = process.env.NODE_ENV): boolean {
  const url = new URL(env.BETTER_AUTH_URL);
  const https = url.protocol === "https:";
  if (nodeEnv === "production" && !https && !LOCAL_HOSTS.has(url.hostname)) {
    throw new Error("BETTER_AUTH_URL must be https in production (session cookies are Secure)");
  }
  return https;
}
