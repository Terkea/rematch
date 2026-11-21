import { account, createDb, type Db, session, user, verification } from "@ci/db";
import { type BetterAuthOptions, betterAuth } from "better-auth";
import { drizzleAdapter } from "better-auth/adapters/drizzle";
import { APIError } from "better-auth/api";
import { genericOAuth, magicLink } from "better-auth/plugins";
import { eq } from "drizzle-orm";
import { getEnv, type WebEnv } from "../env";
import { authDbHooks } from "./auth/db-hooks";
import { googleProvider } from "./auth/google";
import { authHooks } from "./auth/hooks";
import { isInvited, NOT_INVITED } from "./auth/invite";
import { oauthProviders } from "./auth/oauth-providers";
import { magicLinkEmail, sendMail } from "./mail";

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
          const [owner] = await db
            .select({ email: user.email })
            .from(user)
            .where(eq(user.id, data.userId));
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
    advanced: {
      database: { generateId: "uuid" },
      cookiePrefix: "ci",
    },
    account: {
      encryptOAuthTokens: true,
      accountLinking: { enabled: true, trustedProviders: ["google"] },
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
      // Invite-only sign-up (ADR 0041): every method (magic link, Google, generic OAuth) creates
      // users through here. Linking and returning sign-ins are covered by the session guard.
      async validateUserInfo({ user: candidate, source }) {
        if (source.action !== "create-user") return;
        if (await isInvited(db, env, String(candidate.email ?? ""))) return;
        return { error: NOT_INVITED, errorDescription: "This address cannot sign in yet." };
      },
    },
    socialProviders: googleProvider(env),
    plugins: [
      magicLink({
        expiresIn: 600,
        // The response is the same whether or not the address is invited; only invited addresses
        // get mail, so the endpoint reveals nothing about invites or accounts.
        async sendMagicLink({ email, url }) {
          if (!(await isInvited(db, env, email))) return;
          await sendMail(magicLinkEmail(email, url));
        },
      }),
      genericOAuth({ config: oauthProviders(env) }),
    ],
    hooks: authHooks,
    databaseHooks: withInviteGuard(authDbHooks, db, env),
  });
}

export type Auth = ReturnType<typeof createAuth>;

let instance: Auth | undefined;
let instanceDb: { url: string; db: Db } | undefined;

/** The app's Better Auth instance, built on first use from the validated environment. */
export function getAuth(): Auth {
  if (instance) return instance;
  const env = getEnv();
  if (instanceDb?.url !== env.DATABASE_URL) {
    instanceDb = { url: env.DATABASE_URL, db: createDb(env.DATABASE_URL) };
  }
  instance = createAuth(env, instanceDb.db);
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
    const error = new URL(location, baseURL).searchParams.get("error")?.toLowerCase();
    if (error === NOT_INVITED) return redirectTo(res, signIn("not_invited"));
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
  const res = await getAuth().handler(req);
  return mapAuthResponse(req, res, getEnv().BETTER_AUTH_URL);
}
