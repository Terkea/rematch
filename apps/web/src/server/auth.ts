import { account, type Db, session, user, verification } from "@ci/db";
import { type BetterAuthOptions, betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { APIError } from "better-auth/api";
import { genericOAuth, magicLink } from "better-auth/plugins";
import { eq } from "drizzle-orm";
import { getEnv, type WebEnv } from "../env";
import { runAfterResponse } from "./auth/background";
import { authDbHooks } from "./auth/db-hooks";
import { googleProvider } from "./auth/google";
import { authHooks } from "./auth/hooks";
import { isInvited, NOT_INVITED } from "./auth/invite";
import { oauthProviders } from "./auth/oauth-providers";
import { withPendingEmail } from "./auth/pending-email";
import {
  ACCOUNT_NOT_LINKED,
  admissionPolicy,
  deliverMagicLink,
  EMAIL_NOT_VERIFIED,
  ipAddressOptions,
  secureCookies,
} from "./auth/policy";
import { getDb } from "./db";

type DbHooks = NonNullable<BetterAuthOptions["databaseHooks"]>;

const notInvited = () =>
  new APIError("FORBIDDEN", { code: NOT_INVITED, message: "This address cannot sign in yet." });

/**
 * Wraps the app's database hooks with the invite guard for sessions: a returning user whose invite
 * was withdrawn gets no new session. (New accounts are gated by `user.validateUserInfo`.)
 */
function withInviteGuard(hooks: DbHooks, db: Db, env: WebEnv): DbHooks {
  const next = hooks.session?.create?.before;
  return {
    ...hooks,
    session: {
      ...hooks.session,
      create: {
        ...hooks.session?.create,
        async before(data, ctx) {
          const owner = ctx
            ? await ctx.context.internalAdapter.findUserById(data.userId)
            : (await db.select().from(user).where(eq(user.id, data.userId)))[0];
          if (!owner || !(await isInvited(db, env, owner.email))) throw notInvited();
          return next ? next(data, ctx) : undefined;
        },
      },
    },
  };
}

export function createAuth(env: WebEnv, db: Db) {
  return betterAuth({
    appName: "chess-improver",
    baseURL: env.BETTER_AUTH_URL,
    secret: env.BETTER_AUTH_SECRET,
    trustedOrigins: env.BETTER_AUTH_TRUSTED_ORIGINS.split(",")
      .map((origin) => origin.trim())
      .filter(Boolean),
    database: drizzleAdapter(db, {
      provider: "pg",
      schema: { user, session, account, verification },
    }),
    // Never mounted (ADR-WEB-4, review I1): they hand stored OAuth tokens (decrypted) or provider
    // profiles to the browser. Unlinking goes through DELETE /api/accounts/:linkId instead.
    disabledPaths: ["/get-access-token", "/refresh-token", "/account-info", "/unlink-account"],
    advanced: {
      database: { generateId: "uuid" },
      cookiePrefix: "ci",
      useSecureCookies: secureCookies(env),
      // Better Auth skips origin and CSRF checks under NODE_ENV=test; keep them on everywhere so
      // tests exercise the production behaviour.
      disableOriginCheck: false,
      disableCSRFCheck: false,
      ipAddress: ipAddressOptions(env),
    },
    account: {
      encryptOAuthTokens: true,
      accountLinking: {
        enabled: true,
        // Lichess is trusted only so that a live session may link it although its email is never
        // verified; the admission policy refuses every implicit (by-email) Lichess link
        // (ADR-WEB-4). Different emails: a player's Lichess email need not match their sign-in
        // address, and an explicit link is proven by the session plus the OAuth exchange.
        trustedProviders: ["google", "lichess"],
        allowDifferentEmails: true,
      },
    },
    rateLimit: {
      // On in every environment: the magic-link limit is part of the contract (and tested).
      enabled: true,
      storage: "memory",
      customRules: { "/sign-in/magic-link": { window: 600, max: 3 } },
    },
    user: {
      additionalFields: {
        birthMonth: { type: "string", required: false, input: false },
        ageCheckedAt: { type: "date", required: false, input: false },
        marketingEmail: { type: "boolean", required: false, defaultValue: false, input: false },
        publicShareDefault: { type: "boolean", required: false, defaultValue: false, input: false },
      },
      // Verified-only OAuth and invite-only sign-up (ADR 0041): every method creates users here.
      validateUserInfo: admissionPolicy(db, env),
    },
    socialProviders: googleProvider(env),
    plugins: [
      magicLink({
        expiresIn: 600,
        storeToken: "hashed",
        // Always answers the same way: delivery (invite check, per-email throttle, SMTP) runs
        // after the response, so neither the body nor the timing tells addresses apart.
        sendMagicLink({ email, url }) {
          runAfterResponse("magic link", () => deliverMagicLink(db, env, email, url));
        },
      }),
      genericOAuth({ config: oauthProviders(env) }),
    ],
    hooks: authHooks,
    databaseHooks: withInviteGuard(authDbHooks(db), db, env),
  });
}

export type Auth = ReturnType<typeof createAuth>;

let instance: Auth | undefined;

/** Better Auth shares the process's one pool. */
export function getAuthDb(): Db {
  return getDb();
}

/** The app's Better Auth instance, built on first use from the validated environment. */
export function getAuth(): Auth {
  if (instance) return instance;
  const env = getEnv();
  instance = createAuth(env, getAuthDb());
  return instance;
}

/** Drops the cached instance so the next `getAuth()` re-reads the environment (tests). */
export function resetAuth(): void {
  instance = undefined;
}

/**
 * `auth` for the Better Auth CLI and for callers that want the instance itself. It is built lazily
 * so that importing this module (for example during `next build`) needs no environment.
 */
export const auth: Auth = new Proxy({} as Auth, {
  get: (_target, key) => Reflect.get(getAuth(), key),
  has: (_target, key) => Reflect.has(getAuth(), key),
});

const TOKEN_ERRORS = new Set(["invalid_token", "expired_token", "attempts_exceeded"]);

function redirectTo(res: Response, location: string): Response {
  const headers = new Headers(res.headers);
  headers.set("location", location);
  headers.delete("content-type");
  headers.delete("content-length");
  return new Response(null, { status: 302, headers });
}

/** Better Auth's own error page, where an OAuth error lands without an `errorCallbackURL`. */
const DEFAULT_ERROR_PATH = "/api/auth/error";

/** Error codes the general mapping below turns into the sign-in page's own messages. */
const GENERAL_ERRORS = new Set([NOT_INVITED, EMAIL_NOT_VERIFIED]);

/**
 * Lichess callback errors (ADR-WEB-4). Each failure gets its own code on the page the flow returns
 * to (the sign-in page, or Accounts for a link), never a preset one:
 * - refused consent (`access_denied`) becomes `lichess_denied`;
 * - a Lichess account already connected to another user becomes `lichess_taken`;
 * - an implicit by-email link goes to the sign-in page's `account_not_linked`;
 * - anything else (a missing, stale or replayed state, a failed exchange) becomes
 *   `lichess_failed`; Better Auth's raw error page is replaced by the sign-in page.
 * `not_invited` and `email_not_verified` are left to the general mapping.
 */
function mapLichessError(target: URL, error: string, baseURL: string): string | null {
  if (GENERAL_ERRORS.has(error)) return null;
  const code =
    error === "access_denied"
      ? "lichess_denied"
      : error === "account_already_linked_to_different_user"
        ? "lichess_taken"
        : error === ACCOUNT_NOT_LINKED
          ? ACCOUNT_NOT_LINKED
          : "lichess_failed";
  if (target.pathname === DEFAULT_ERROR_PATH || code === ACCOUNT_NOT_LINKED) {
    return new URL(`/sign-in?error=${code}`, baseURL).toString();
  }
  const next = new URL(target);
  next.searchParams.delete("error");
  next.searchParams.delete("error_description");
  next.searchParams.set("error", code);
  return next.toString();
}

/**
 * Maps Better Auth's error redirects to the app's sign-in messages (Task 21 copy):
 * a used, expired or unknown magic link goes to `/sign-in?error=link_expired`, and an address
 * that is not invited to `/sign-in?error=not_invited`, from any sign-in method.
 */
export async function mapAuthResponse(
  req: Request,
  res: Response,
  baseURL: string,
): Promise<Response> {
  const path = new URL(req.url).pathname;
  const isVerify = path.endsWith("/magic-link/verify");
  const signIn = (error: string) => new URL(`/sign-in?error=${error}`, baseURL).toString();
  const location = res.headers.get("location");
  if (res.status >= 300 && res.status < 400 && location) {
    const target = new URL(location, baseURL);
    // Better Auth appends its `error`, so the last one is the real one.
    const error = target.searchParams.getAll("error").at(-1)?.toLowerCase();
    if (path.endsWith("/callback/lichess") && error) {
      const lichess = mapLichessError(target, error, baseURL);
      if (lichess) return redirectTo(res, lichess);
    }
    if (error === NOT_INVITED) return redirectTo(res, signIn("not_invited"));
    if (error === EMAIL_NOT_VERIFIED) return redirectTo(res, signIn(EMAIL_NOT_VERIFIED));
    if (isVerify && error && TOKEN_ERRORS.has(error))
      return redirectTo(res, signIn("link_expired"));
    return res;
  }
  if (isVerify && res.status === 403) {
    const body = (await res
      .clone()
      .json()
      .catch(() => null)) as { code?: string } | null;
    if (body?.code?.toLowerCase() === NOT_INVITED) return redirectTo(res, signIn("not_invited"));
  }
  return res;
}

/** The `/api/auth/*` route handler. */
export async function handleAuthRequest(req: Request): Promise<Response> {
  const env = getEnv();
  // Read before the handler consumes the body: a successful send carries its address to the
  // check-email page in a cookie, never in the URL (pending-email.ts).
  const copy = req.clone();
  const res = await getAuth().handler(req);
  return withPendingEmail(
    copy,
    await mapAuthResponse(req, res, env.BETTER_AUTH_URL),
    secureCookies(env),
  );
}
