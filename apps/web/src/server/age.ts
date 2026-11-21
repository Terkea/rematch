import { type Db, invites, user } from "@ci/db";
import { eq, sql } from "drizzle-orm";
import type { AppUser } from "./auth/session";

/**
 * The age gate (ADR-WEB-6). We keep the month and year of birth only (`YYYY-MM`), never a full
 * date, and do the arithmetic conservatively so an age is never overstated.
 */

export type AgeBand = "under13" | "teen" | "adult";
export type AgeRedirect = "/app/accounts" | "/blocked";

/** The essential cookie that stops a blocked browser from signing in again (30 days). */
export const AGE_BLOCK_COOKIE = "ci_age_block";
export const AGE_BLOCK_MAX_AGE = 30 * 24 * 3600;

const EARLIEST_YEAR = 1900;
const BIRTH_MONTH = /^(\d{4})-(0[1-9]|1[0-2])$/;

export class InvalidBirthMonthError extends Error {
  constructor(message = "Enter a real month and year of birth.") {
    super(message);
    this.name = "InvalidBirthMonthError";
  }
}

/** The age was already recorded; it is answered once and cannot be changed through the gate. */
export class AgeAlreadyRecordedError extends Error {
  constructor() {
    super("The age for this account is already recorded.");
    this.name = "AgeAlreadyRecordedError";
  }
}

/**
 * Validates a month and year of birth and returns `"YYYY-MM"`. Throws `InvalidBirthMonthError`
 * for a non-integer, a month outside 1–12, a year before 1900, or a month after `today`'s (UTC).
 */
export function parseBirthMonth(input: { year: number; month: number }, today: Date): string {
  const { year, month } = input;
  if (!Number.isInteger(year) || !Number.isInteger(month)) throw new InvalidBirthMonthError();
  if (month < 1 || month > 12 || year < EARLIEST_YEAR) throw new InvalidBirthMonthError();
  const nowYear = today.getUTCFullYear();
  const nowMonth = today.getUTCMonth() + 1;
  if (year > nowYear || (year === nowYear && month > nowMonth)) {
    throw new InvalidBirthMonthError();
  }
  return `${year}-${String(month).padStart(2, "0")}`;
}

/** The last day (1-based) of a month, e.g. 29 for February 2012. */
function lastDayOf(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

/**
 * Age in whole years on `today` (UTC), assuming the birthday is the last day of the birth month,
 * so the result never overstates the age. A February birthday in a leap year counts from the 29th,
 * which in other years means from 1 March.
 */
export function ageFromBirthMonth(birthMonth: string, today: Date): number {
  const match = BIRTH_MONTH.exec(birthMonth);
  if (!match) throw new InvalidBirthMonthError(`Not a YYYY-MM birth month: ${birthMonth}`);
  const year = Number(match[1]);
  const month = Number(match[2]);
  const nowMonth = today.getUTCMonth() + 1;
  const nowDay = today.getUTCDate();
  let age = today.getUTCFullYear() - year;
  if (nowMonth < month || (nowMonth === month && nowDay < lastDayOf(year, month))) age -= 1;
  return age;
}

export function ageBand(age: number): AgeBand {
  if (age < 13) return "under13";
  if (age < 18) return "teen";
  return "adult";
}

/** True for under-18s, and for anyone whose age is not yet known (the private default). */
export function isMinor(person: AppUser, today: Date): boolean {
  if (person.birthMonth === null) return true;
  return ageFromBirthMonth(person.birthMonth, today) < 18;
}

/** Marketing email is opt-in, and only adults may opt in. */
export function canEnableMarketing(person: AppUser, today: Date): boolean {
  return !isMinor(person, today);
}

/**
 * Records the answer to the age screen, once per account.
 * - Under 13: deletes the account at once (sessions, OAuth accounts, account links and chess.com
 *   budgets cascade), plus every other copy of the address: its invite and its send log. The shared
 *   `linked_accounts` rows stay, because they are public platform accounts other users may link.
 * - Teen: stores the month and forces both preferences off.
 * - Adult: stores the month; both preferences keep their opt-in default (off).
 * Throws `InvalidBirthMonthError` before touching the database, and `AgeAlreadyRecordedError` when
 * the account already has an age (so the gate cannot be used to change it, or to delete an account).
 */
export async function recordAge(
  deps: { db: Db },
  userId: string,
  input: { year: number; month: number },
  now: Date,
): Promise<{ band: AgeBand; redirectTo: AgeRedirect }> {
  const birthMonth = parseBirthMonth(input, now);
  const band = ageBand(ageFromBirthMonth(birthMonth, now));
  await deps.db.transaction(async (tx) => {
    const [row] = await tx
      .select({ email: user.email, birthMonth: user.birthMonth })
      .from(user)
      .where(eq(user.id, userId))
      .for("update");
    if (!row || row.birthMonth !== null) throw new AgeAlreadyRecordedError();
    if (band === "under13") {
      await tx.delete(invites).where(eq(invites.email, row.email));
      await tx.execute(sql`delete from magic_link_sends where email = ${row.email}`);
      await tx.delete(user).where(eq(user.id, userId));
      return;
    }
    await tx
      .update(user)
      .set(
        band === "teen"
          ? { birthMonth, ageCheckedAt: now, marketingEmail: false, publicShareDefault: false }
          : { birthMonth, ageCheckedAt: now },
      )
      .where(eq(user.id, userId));
  });
  return { band, redirectTo: band === "under13" ? "/blocked" : "/app/accounts" };
}

const GATED_PREFIXES = ["/sign-in/", "/sign-up/", "/magic-link/", "/oauth2/", "/callback/"];

/** The Better Auth paths that start or finish a sign-in, which a blocked browser may not use. */
export function isAgeGatedPath(path: string): boolean {
  return GATED_PREFIXES.some((prefix) => path.startsWith(prefix));
}

/** True when the request's `Cookie` header carries the age block. */
export function hasAgeBlock(headers: Headers | undefined): boolean {
  const cookie = headers?.get("cookie");
  if (!cookie) return false;
  return cookie.split(";").some((part) => part.trim().startsWith(`${AGE_BLOCK_COOKIE}=`));
}
