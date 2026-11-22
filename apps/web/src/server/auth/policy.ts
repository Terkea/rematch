import type { Db } from "@ci/db";
import type { BetterAuthOptions } from "better-auth";
import { getSessionFromCtx } from "better-auth/api";
import { sql } from "drizzle-orm";
import type { WebEnv } from "../../env";
import { magicLinkEmail, sendMail } from "../mail";
import { isInvited, NOT_INVITED, normaliseEmail } from "./invite";
import { isLichessPlaceholderEmail } from "./lichess";

/** Magic links sent to one address per window (on top of the per-IP limit). */
export const MAGIC_LINKS_PER_EMAIL = 3;
export const MAGIC_LINK_WINDOW_SECONDS = 600;

/** Error code for an implicit (by-email) link of a provider that does not assert email. */
export const ACCOUNT_NOT_LINKED = "account_not_linked";

/** Error code for an OAuth identity whose email the provider has not verified. */
export const EMAIL_NOT_VERIFIED = "email_not_verified";

type ValidateUserInfo = NonNullable<NonNullable<BetterAuthOptions["user"]>["validateUserInfo"]>;
type EndpointContext = Parameters<ValidateUserInfo>[1];

/**
 * OAuth providers whose email does not prove anything about the person (Lichess, chess.com,
 * ADR-WEB-4): they link to an existing account only from that account's own live session (never
 * implicitly by email), and under INVITE_ONLY their email cannot prove an invite, so they cannot
 * create accounts. Invited players sign in by email and connect Lichess from Accounts.
 */
export const NON_EMAIL_PROVIDERS: ReadonlySet<string> = new Set(["lichess", "chesscom"]);

/** The id of the user signed in on this request, if any. */
export type SessionUserId = (ctx: EndpointContext) => Promise<string | null>;

const sessionUserIdFromCtx: SessionUserId = async (ctx) => {
  try {
    return (await getSessionFromCtx(ctx, { disableRefresh: true }))?.user.id ?? null;
  } catch {
    return null;
  }
};

/**
 * Admission policy for every sign-in method (ADR-WEB-3):
 * - an identity from an email-asserting OAuth provider (Google) may create an account, or be
 *   linked implicitly to the account with that email, only with a provider-verified email, so a
 *   provider account that merely claims an invited address cannot take that account over (Google
 *   is a trusted linking provider, which skips Better Auth's own verified check). Linking started
 *   by the account's own live session is exempt: there the session, not the email, grants trust;
 * - with INVITE_ONLY on, only invited addresses may create an account.
 */
export function admissionPolicy(
  db: Db,
  env: WebEnv,
  sessionUserId: SessionUserId = sessionUserIdFromCtx,
): ValidateUserInfo {
  return async ({ user, source }, ctx) => {
    const creates = source.action === "create-user";
    if (source.method === "oauth") {
      const providerId = source.oauth?.providerId ?? "";
      if (NON_EMAIL_PROVIDERS.has(providerId)) {
        if (creates && env.INVITE_ONLY) {
          return { error: NOT_INVITED, errorDescription: "This address cannot sign in yet." };
        }
        if (source.action === "link-account") {
          const fromLiveSession =
            typeof user.id === "string" && (await sessionUserId(ctx)) === user.id;
          if (fromLiveSession) return;
          return {
            error: ACCOUNT_NOT_LINKED,
            errorDescription: "Sign in first, then connect this account from Accounts.",
          };
        }
        if (!creates) return;
      } else if (user.emailVerified !== true) {
        const fromLiveSession =
          source.action === "link-account" &&
          typeof user.id === "string" &&
          (await sessionUserId(ctx)) === user.id;
        if ((creates || source.action === "link-account") && !fromLiveSession) {
          return {
            error: EMAIL_NOT_VERIFIED,
            errorDescription: "The provider has not verified this email.",
          };
        }
      }
    }
    if (!creates) return;
    if (await isInvited(db, env, String(user.email ?? ""))) return;
    return { error: NOT_INVITED, errorDescription: "This address cannot sign in yet." };
  };
}

/** Separate advisory-lock namespace (two-int form) for per-address magic-link decisions. */
const MAGIC_LINK_LOCK_SPACE = 7_262_013;

/**
 * Records a send for this address if it is under the throttle. Count, prune and insert run under
 * a per-address transaction lock, so a burst of N requests sends exactly min(N, 3) links, and a
 * throttled request records nothing (it never extends the lockout).
 */
async function claimMagicLinkSend(db: Db, email: string): Promise<boolean> {
  const window = sql`now() - make_interval(secs => ${MAGIC_LINK_WINDOW_SECONDS})`;
  return db.transaction(async (tx) => {
    await tx.execute(
      sql`select pg_advisory_xact_lock(${MAGIC_LINK_LOCK_SPACE}, hashtext(${email}))`,
    );
    const [row] = await tx.execute<{ n: number }>(sql`
      select count(*)::int as n from magic_link_sends where email = ${email} and sent_at > ${window}`);
    if ((row?.n ?? 0) >= MAGIC_LINKS_PER_EMAIL) return false;
    await tx.execute(
      sql`delete from magic_link_sends where email = ${email} and sent_at <= ${window}`,
    );
    await tx.execute(sql`insert into magic_link_sends (email) values (${email})`);
    return true;
  });
}

/**
 * Sends the magic link if the address may have one. Runs after the response (see
 * `runAfterResponse`), so the request path is the same for every address: invited or not,
 * throttled or not, the endpoint answers `200 { status: true }` at the same point.
 * A claimed send whose SMTP delivery then fails still counts towards the throttle.
 */
export async function deliverMagicLink(
  db: Db,
  env: WebEnv,
  email: string,
  url: string,
): Promise<void> {
  const to = normaliseEmail(email);
  // A Lichess-created account's placeholder can never receive a link (ADR-WEB-4, review I5).
  if (!to || isLichessPlaceholderEmail(to)) return;
  if (!(await isInvited(db, env, to))) return;
  if (!(await claimMagicLinkSend(db, to))) return;
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
