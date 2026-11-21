import { randomInt } from "node:crypto";
import { accountLinks, chesscomVerifications, type Db, linkedAccounts } from "@ci/db";
import { and, eq, isNotNull, isNull, lte, ne, or, sql } from "drizzle-orm";
import type { WebEnv } from "../../env";
import { type ChesscomDeps, fetchChesscomProfile } from "../chesscom/profile";

/** Verification code alphabet: no 0/O, 1/I/L (ADR-WEB-5). */
export const CODE_ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ";
export const CODE_LENGTH = 6;
export const CODE_TTL_MS = 7 * 24 * 3600 * 1000;
export const CHECK_COOLDOWN_MS = 60_000;
export const MAX_CHESSCOM_LINKS = 3;
export const USERNAME_PATTERN = /^[A-Za-z0-9_-]{3,25}$/;

export type LinkErrorCode = "invalid_username" | "not_found" | "limit_reached" | "link_not_found";

export class LinkError extends Error {
  constructor(readonly code: LinkErrorCode) {
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

/** `verified` is true only when asking again for a link that is already verified. */
export type StartedLink = { linkId: string; username: string; code: string; verified: boolean };
export type VerificationResult = { status: "verified" | "pending" | "expired"; nextCheckAt: Date };

/** Advisory-lock namespaces (two-int form): per user for linking, per account for ownership. */
const LINK_LOCK_SPACE = 7_262_014;
const OWNER_LOCK_SPACE = 7_262_015;

const cryptoRandom = () => randomInt(0, 2 ** 32) / 2 ** 32;

/** `CI-` plus 6 characters from `CODE_ALPHABET`. `rng` returns values in [0, 1). */
export function generateVerificationCode(rng: () => number = cryptoRandom): string {
  let code = "CI-";
  for (let i = 0; i < CODE_LENGTH; i++) {
    const index = Math.min(Math.floor(rng() * CODE_ALPHABET.length), CODE_ALPHABET.length - 1);
    code += CODE_ALPHABET[index];
  }
  return code;
}

/** Route dependencies from the environment. */
export function linkDeps(db: Db, env: WebEnv): LinkDeps {
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

/**
 * Starts (or resumes) linking a chess.com username to this user: the account must exist on
 * chess.com, the user may hold at most 3 chess.com links, and asking again for the same name
 * returns the same link and code (a fresh code once the old one has expired). The link is
 * unverified until the code shows up in the profile's location (`checkChesscomVerification`).
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

  // Fast path, no request to chess.com: the link and its code already exist.
  const existing = await findOwnLink(deps.db, userId, name);
  if (existing?.link.verifiedAt) {
    return {
      ...started(existing.link.id, name, existing.verification?.code ?? ""),
      verified: true,
    };
  }
  if (existing?.verification && existing.verification.expiresAt > now) {
    return started(existing.link.id, name, existing.verification.code);
  }
  if (!existing && (await countChesscomLinks(deps.db, userId)) >= MAX_CHESSCOM_LINKS) {
    throw new LinkError("limit_reached");
  }

  const profile = await fetchChesscomProfile(name, deps.chesscom);
  if (!profile) throw new LinkError("not_found");

  return deps.db.transaction(async (tx) => {
    await tx.execute(sql`select pg_advisory_xact_lock(${LINK_LOCK_SPACE}, hashtext(${userId}))`);
    const current = await findOwnLink(tx, userId, profile.username);
    if (current?.verification && current.verification.expiresAt > now) {
      return started(current.link.id, profile.username, current.verification.code);
    }
    if (!current && (await countChesscomLinks(tx, userId)) >= MAX_CHESSCOM_LINKS) {
      throw new LinkError("limit_reached");
    }

    await tx
      .insert(linkedAccounts)
      .values({
        platform: "chesscom",
        username: profile.username,
        displayName: profile.displayName,
      })
      .onConflictDoNothing({ target: [linkedAccounts.platform, linkedAccounts.username] });
    const [shared] = await tx
      .select({ id: linkedAccounts.id })
      .from(linkedAccounts)
      .where(
        and(eq(linkedAccounts.platform, "chesscom"), eq(linkedAccounts.username, profile.username)),
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
    return started(linkId, profile.username, code);
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
 * Checks whether the link's code is in the chess.com profile's location, at most once a minute
 * per link (one profile request; the cooldown is claimed atomically, so concurrent checks make
 * one request). On a match the link becomes the account's verified owner, and any other user's
 * verified link to the same account is cleared in the same transaction: the newest verification
 * wins, because only the current account holder can edit the location (ADR-WEB-5).
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
  if (!verification || verification.expiresAt <= now)
    return { status: "expired", nextCheckAt: now };

  const cooldownEnds = (from: Date) => new Date(from.getTime() + CHECK_COOLDOWN_MS);
  if (verification.lastCheckedAt && cooldownEnds(verification.lastCheckedAt) > now) {
    return { status: "pending", nextCheckAt: cooldownEnds(verification.lastCheckedAt) };
  }
  const threshold = new Date(now.getTime() - CHECK_COOLDOWN_MS);
  const [claimed] = await deps.db
    .update(chesscomVerifications)
    .set({ lastCheckedAt: now, checks: sql`${chesscomVerifications.checks} + 1` })
    .where(
      and(
        eq(chesscomVerifications.id, verification.id),
        or(
          isNull(chesscomVerifications.lastCheckedAt),
          lte(chesscomVerifications.lastCheckedAt, threshold),
        ),
      ),
    )
    .returning({ id: chesscomVerifications.id });
  if (!claimed) return { status: "pending", nextCheckAt: cooldownEnds(now) };

  const profile = await fetchChesscomProfile(row.account.username, deps.chesscom);
  if (!profile || !locationHasCode(profile.location, verification.code)) {
    return { status: "pending", nextCheckAt: cooldownEnds(now) };
  }

  await deps.db.transaction(async (tx) => {
    await tx.execute(
      sql`select pg_advisory_xact_lock(${OWNER_LOCK_SPACE}, hashtext(${row.account.id}))`,
    );
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
  });
  return { status: "verified", nextCheckAt: now };
}
