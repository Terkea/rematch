import { randomInt } from "node:crypto";
import { accountLinks, chesscomVerifications, type Db, linkedAccounts } from "@ci/db";
import { and, eq, isNotNull, ne, sql } from "drizzle-orm";
import type { WebEnv } from "../../env";
import {
  assertChesscomBaseUrl,
  type ChesscomDeps,
  fetchChesscomProfile,
} from "../chesscom/profile";
import { claimProfileRequest, USERNAME_INTERVAL_MS } from "../chesscom/throttle";

/** Verification code alphabet: no 0/O, 1/I/L (ADR-WEB-5). */
export const CODE_ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
export const CODE_LENGTH = 6;
export const CODE_TTL_MS = 7 * 24 * 3600 * 1000;
export const MAX_CHESSCOM_LINKS = 3;
export const USERNAME_PATTERN = /^[A-Za-z0-9_-]{3,25}$/;

export type LinkErrorCode =
  | "invalid_username"
  | "not_found"
  | "limit_reached"
  | "link_not_found"
  | "rate_limited";

export class LinkError extends Error {
  constructor(
    readonly code: LinkErrorCode,
    /** For `rate_limited`: when a request may be made again. */
    readonly retryAt?: Date,
  ) {
    super(code);
    this.name = "LinkError";
  }
}

export type LinkDeps = {
  db: Db;
  chesscom: ChesscomDeps;
  now?: () => Date;
  /** Tests pin the code; production uses `generateVerificationCode()`. */
  generateCode?: () => string;
};

/**
 * `code` is the pending code; it is null for a link that is already verified (codes are
 * single-use, so an old one is never handed back).
 */
export type StartedLink = {
  linkId: string;
  username: string;
  code: string | null;
  verified: boolean;
};
export type VerificationResult = { status: "verified" | "pending" | "expired"; nextCheckAt: Date };

/** Advisory-lock namespaces (two-int form): per user for linking, per account for ownership. */
const LINK_LOCK_SPACE = 7_262_014;
export const OWNER_LOCK_SPACE = 7_262_015;

/**
 * `CI-` plus 6 characters from `CODE_ALPHABET`. `pick(n)` returns an index in [0, n); the default
 * is `crypto.randomInt(0, n)`, which is unbiased.
 */
export function generateVerificationCode(
  pick: (n: number) => number = (n) => randomInt(0, n),
): string {
  let code = "CI-";
  for (let i = 0; i < CODE_LENGTH; i++) code += CODE_ALPHABET[pick(CODE_ALPHABET.length)];
  return code;
}

/** Route dependencies from the environment. The API base must be https outside dev and tests. */
export function linkDeps(db: Db, env: WebEnv): LinkDeps {
  assertChesscomBaseUrl(env.CHESSCOM_API_BASE_URL);
  return { db, chesscom: { fetch: globalThis.fetch, baseUrl: env.CHESSCOM_API_BASE_URL } };
}

type Tx = Parameters<Parameters<Db["transaction"]>[0]>[0];

async function findOwnLink(db: Db | Tx, userId: string, username: string) {
  const [row] = await db
    .select({ link: accountLinks, verification: chesscomVerifications })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .leftJoin(chesscomVerifications, eq(chesscomVerifications.accountLinkId, accountLinks.id))
    .where(
      and(
        eq(accountLinks.userId, userId),
        eq(linkedAccounts.platform, "chesscom"),
        eq(linkedAccounts.username, username),
      ),
    );
  return row;
}

async function countChesscomLinks(db: Db | Tx, userId: string): Promise<number> {
  const [row] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .where(and(eq(accountLinks.userId, userId), eq(linkedAccounts.platform, "chesscom")));
  return row?.n ?? 0;
}

async function knownAccount(db: Db, username: string) {
  const [row] = await db
    .select({ username: linkedAccounts.username, displayName: linkedAccounts.displayName })
    .from(linkedAccounts)
    .where(and(eq(linkedAccounts.platform, "chesscom"), eq(linkedAccounts.username, username)));
  return row;
}

/** Spends the user's and the username's throttle, or throws `rate_limited`. */
async function claimOrThrow(deps: LinkDeps, userId: string, username: string, now: Date) {
  const claim = await claimProfileRequest(deps.db, userId, username, now);
  if (!claim.ok) throw new LinkError("rate_limited", claim.retryAt);
}

/**
 * Starts (or resumes) linking a chess.com username to this user: the account must exist on
 * chess.com, the user may hold at most 3 chess.com links, and asking again for the same name
 * returns the same link and pending code (a fresh code once the old one has expired or was used).
 * A username some user has already linked is known to exist, so it needs no request; any other
 * name costs one throttled profile request. The link is unverified until the code shows up in
 * the profile's location (`checkChesscomVerification`).
 */
export async function startChesscomLink(
  deps: LinkDeps,
  userId: string,
  username: string,
): Promise<StartedLink> {
  if (typeof username !== "string" || !USERNAME_PATTERN.test(username)) {
    throw new LinkError("invalid_username");
  }
  const name = username.toLowerCase();
  const now = deps.now?.() ?? new Date();
  const newCode = deps.generateCode ?? (() => generateVerificationCode());

  const existing = await findOwnLink(deps.db, userId, name);
  if (existing?.link.verifiedAt) {
    return { linkId: existing.link.id, username: name, code: null, verified: true };
  }
  if (existing?.verification && existing.verification.expiresAt > now) {
    return started(existing.link.id, name, existing.verification.code);
  }
  if (!existing && (await countChesscomLinks(deps.db, userId)) >= MAX_CHESSCOM_LINKS) {
    throw new LinkError("limit_reached");
  }

  let profile = await knownAccount(deps.db, name);
  if (!profile) {
    await claimOrThrow(deps, userId, name, now);
    const fetched = await fetchChesscomProfile(name, deps.chesscom);
    if (!fetched) throw new LinkError("not_found");
    profile = fetched;
  }
  const account = profile;

  return deps.db.transaction(async (tx) => {
    await tx.execute(sql`select pg_advisory_xact_lock(${LINK_LOCK_SPACE}, hashtext(${userId}))`);
    const current = await findOwnLink(tx, userId, account.username);
    if (current?.link.verifiedAt) {
      return { linkId: current.link.id, username: account.username, code: null, verified: true };
    }
    if (current?.verification && current.verification.expiresAt > now) {
      return started(current.link.id, account.username, current.verification.code);
    }
    if (!current && (await countChesscomLinks(tx, userId)) >= MAX_CHESSCOM_LINKS) {
      throw new LinkError("limit_reached");
    }

    await tx
      .insert(linkedAccounts)
      .values({
        platform: "chesscom",
        username: account.username,
        displayName: account.displayName,
      })
      .onConflictDoNothing({ target: [linkedAccounts.platform, linkedAccounts.username] });
    const [shared] = await tx
      .select({ id: linkedAccounts.id })
      .from(linkedAccounts)
      .where(
        and(eq(linkedAccounts.platform, "chesscom"), eq(linkedAccounts.username, account.username)),
      );
    if (!shared) throw new Error("startChesscomLink: linked account vanished");

    let linkId = current?.link.id;
    if (!linkId) {
      const [link] = await tx
        .insert(accountLinks)
        .values({ userId, linkedAccountId: shared.id, method: "username" })
        .returning({ id: accountLinks.id });
      if (!link) throw new Error("startChesscomLink: link insert returned no row");
      linkId = link.id;
    }

    const code = newCode();
    const expiresAt = new Date(now.getTime() + CODE_TTL_MS);
    await tx
      .insert(chesscomVerifications)
      .values({ accountLinkId: linkId, code, expiresAt })
      .onConflictDoUpdate({
        target: chesscomVerifications.accountLinkId,
        set: { code, expiresAt, lastCheckedAt: null, checks: 0, createdAt: now },
      });
    return started(linkId, account.username, code);
  });
}

function started(linkId: string, username: string, code: string): StartedLink {
  return { linkId, username, code, verified: false };
}

/** True when `code` appears, case-insensitively, anywhere in the trimmed location. */
export function locationHasCode(location: string | null, code: string): boolean {
  if (!location) return false;
  return location.trim().toUpperCase().includes(code.toUpperCase());
}

/**
 * Checks whether the link's code is in the chess.com profile's location. Each check is one
 * throttled profile request: at most one per username per 60 s across all users (the answer is
 * then `pending` with `nextCheckAt`), and within the user's 10-per-hour bucket (`rate_limited`).
 *
 * On a match the link becomes the account's verified owner and its code is used up; any other
 * user's verified link to the same account is cleared in the same transaction. The newest
 * verification wins, because only the current account holder can edit the location (ADR-WEB-5).
 * The transaction re-reads the link under a lock, so a link verified meanwhile, or a code
 * reissued meanwhile, is not overwritten with a stale result.
 */
export async function checkChesscomVerification(
  deps: LinkDeps,
  userId: string,
  linkId: string,
  now: Date,
): Promise<VerificationResult> {
  const [row] = await deps.db
    .select({ link: accountLinks, account: linkedAccounts, verification: chesscomVerifications })
    .from(accountLinks)
    .innerJoin(linkedAccounts, eq(linkedAccounts.id, accountLinks.linkedAccountId))
    .leftJoin(chesscomVerifications, eq(chesscomVerifications.accountLinkId, accountLinks.id))
    .where(and(eq(accountLinks.id, linkId), eq(accountLinks.userId, userId)));
  if (row?.account.platform !== "chesscom") throw new LinkError("link_not_found");
  if (row.link.verifiedAt) return { status: "verified", nextCheckAt: now };
  const verification = row.verification;
  if (!verification || verification.expiresAt <= now) {
    return { status: "expired", nextCheckAt: now };
  }

  const claim = await claimProfileRequest(deps.db, userId, row.account.username, now);
  if (!claim.ok) {
    if (claim.reason === "username_interval")
      return { status: "pending", nextCheckAt: claim.retryAt };
    throw new LinkError("rate_limited", claim.retryAt);
  }
  await deps.db
    .update(chesscomVerifications)
    .set({ lastCheckedAt: now, checks: sql`${chesscomVerifications.checks} + 1` })
    .where(eq(chesscomVerifications.id, verification.id));

  const nextCheckAt = new Date(now.getTime() + USERNAME_INTERVAL_MS);
  const profile = await fetchChesscomProfile(row.account.username, deps.chesscom);
  if (!profile || !locationHasCode(profile.location, verification.code)) {
    return { status: "pending", nextCheckAt };
  }

  return deps.db.transaction(async (tx): Promise<VerificationResult> => {
    await tx.execute(
      sql`select pg_advisory_xact_lock(${OWNER_LOCK_SPACE}, hashtext(${row.account.id}))`,
    );
    const [fresh] = await tx
      .select({ link: accountLinks, verification: chesscomVerifications })
      .from(accountLinks)
      .innerJoin(chesscomVerifications, eq(chesscomVerifications.accountLinkId, accountLinks.id))
      .where(and(eq(accountLinks.id, linkId), eq(accountLinks.userId, userId)))
      .for("update");
    if (!fresh) throw new LinkError("link_not_found");
    if (fresh.link.verifiedAt) return { status: "verified", nextCheckAt: now };
    if (fresh.verification.code !== verification.code || fresh.verification.expiresAt <= now) {
      return { status: "pending", nextCheckAt };
    }
    await tx
      .update(accountLinks)
      .set({ verifiedAt: null, method: "username" })
      .where(
        and(
          eq(accountLinks.linkedAccountId, row.account.id),
          ne(accountLinks.id, linkId),
          isNotNull(accountLinks.verifiedAt),
        ),
      );
    await tx
      .update(accountLinks)
      .set({ verifiedAt: now, method: "location_code" })
      .where(eq(accountLinks.id, linkId));
    // Single-use: the code is spent, so it is never shown or accepted again.
    await tx
      .update(chesscomVerifications)
      .set({ expiresAt: now })
      .where(eq(chesscomVerifications.id, fresh.verification.id));
    return { status: "verified", nextCheckAt: now };
  });
}
