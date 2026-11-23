import { accountLinks, games, linkedAccounts, user } from "@ci/db";
import { drillItems, reports, weaknessEstimates, weeklyReviews } from "@ci/db/generated";
import { and, eq, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import { DEMO_USERNAME, removeDemo, seedDemo } from "../../scripts/demo-seed";
import { hasDemoAccount, isDemoUsername } from "../../src/server/demo";
import { requestSync } from "../../src/server/sync/request";
import { resetGameTables, seedAccount, seedSyntheticGame } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

// ADR 0097: `pnpm seed:demo` writes a fictional player's analysed games into a local dev database
// that may also hold real accounts, so it must touch only its own rows, be repeatable, and be
// removable without a trace. The trainer, report and weakness rows come from the same synthetic
// fixtures the e2e seeds use.

const db = webTestDb();
const EMAIL = "demo-owner@example.invalid";

async function demoAccountId(): Promise<string | undefined> {
  const [a] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.username, DEMO_USERNAME));
  return a?.id;
}

async function countFor(accountId: string) {
  const n = async (q: ReturnType<typeof sql>) =>
    ((await db.execute(q)) as unknown as { n: number }[])[0]?.n ?? 0;
  return {
    games: await n(sql`select count(*)::int as n from games where account_id = ${accountId}`),
    moves: await n(sql`select count(*)::int as n from moves m join games g on g.id = m.game_id
      where g.account_id = ${accountId}`),
    estimates: await n(
      sql`select count(*)::int as n from weakness_estimates where account_id = ${accountId}`,
    ),
    reports: await n(sql`select count(*)::int as n from reports where account_id = ${accountId}`),
    weekly: await n(
      sql`select count(*)::int as n from weekly_reviews where account_id = ${accountId}`,
    ),
    kpis: await n(sql`select count(*)::int as n from kpi_snapshot where account_id = ${accountId}`),
    drills: await n(
      sql`select count(*)::int as n from drill_items where account_id = ${accountId}`,
    ),
  };
}

let realAccountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await db.execute(sql`truncate table jobs, report_runs, reports cascade`);
  // Someone's real account in the same database: the demo must never touch it.
  const { user: owner } = await createTestSession(db, { email: "owner@example.invalid" });
  ({ accountId: realAccountId } = await seedAccount(db, {
    userId: owner.id,
    username: "realplayer",
  }));
  await seedSyntheticGame(db, realAccountId, {
    playedAt: "2026-10-01T10:00:00.000Z",
    timeClass: "rapid",
    result: "1-0",
  });
});

describe("seedDemo", () => {
  it("creates the user when needed and links them, verified, to the demo player", async () => {
    const out = await seedDemo(db, { email: EMAIL });
    const [u] = await db.select().from(user).where(eq(user.email, EMAIL));
    expect(u).toBeDefined();
    const accountId = await demoAccountId();
    expect(accountId).toBe(out.accountId);
    const [link] = await db
      .select()
      .from(accountLinks)
      .where(
        and(
          eq(accountLinks.userId, u?.id as string),
          eq(accountLinks.linkedAccountId, accountId as string),
        ),
      );
    expect(link?.verifiedAt).not.toBeNull();
  });

  it("gives every page content: analysed games, weaknesses, a report, a weekly review, drills", async () => {
    const { accountId } = await seedDemo(db, { email: EMAIL });
    const c = await countFor(accountId);
    expect(c.games).toBeGreaterThanOrEqual(30);
    expect(c.moves).toBeGreaterThan(100);
    expect(c.estimates).toBeGreaterThanOrEqual(5);
    expect(c.reports).toBe(1);
    expect(c.weekly).toBe(1);
    expect(c.kpis).toBeGreaterThan(10);
    expect(c.drills).toBeGreaterThan(0);
  });

  it("dates the games up to the last few days, so the pages read as current", async () => {
    const { accountId } = await seedDemo(db, { email: EMAIL });
    const [row] = (await db.execute(
      sql`select max(played_at) as newest from games where account_id = ${accountId}`,
    )) as unknown as { newest: Date | string }[];
    const ageDays = (Date.now() - new Date(row?.newest as string).getTime()) / 86_400_000;
    expect(ageDays).toBeGreaterThanOrEqual(0);
    expect(ageDays).toBeLessThan(8);
  });

  it("is idempotent: a second run leaves the same world", async () => {
    const first = await seedDemo(db, { email: EMAIL });
    const before = await countFor(first.accountId);
    const ids = await db
      .select({ id: games.id })
      .from(games)
      .where(eq(games.accountId, first.accountId))
      .orderBy(games.id);
    const second = await seedDemo(db, { email: EMAIL });
    expect(await countFor(second.accountId)).toEqual(before);
    expect(
      await db
        .select({ id: games.id })
        .from(games)
        .where(eq(games.accountId, second.accountId))
        .orderBy(games.id),
    ).toEqual(ids);
    expect((await db.select().from(user).where(eq(user.email, EMAIL))).length).toBe(1);
  });

  it("never touches another account", async () => {
    await seedDemo(db, { email: EMAIL });
    expect((await countFor(realAccountId)).games).toBe(1);
    await removeDemo(db);
    expect((await countFor(realAccountId)).games).toBe(1);
  });

  it("is removable: no demo row is left, the user stays", async () => {
    const { accountId } = await seedDemo(db, { email: EMAIL });
    await removeDemo(db);
    expect(await demoAccountId()).toBeUndefined();
    expect(await countFor(accountId)).toEqual({
      games: 0,
      moves: 0,
      estimates: 0,
      reports: 0,
      weekly: 0,
      kpis: 0,
      drills: 0,
    });
    for (const t of [weaknessEstimates, reports, weeklyReviews, drillItems]) {
      const rows = await db.select().from(t).where(eq(t.accountId, accountId));
      expect(rows).toEqual([]);
    }
    expect((await db.select().from(user).where(eq(user.email, EMAIL))).length).toBe(1);
    // A second removal is a no-op.
    await removeDemo(db);
  });

  it("refuses an address that is not an email", async () => {
    await expect(seedDemo(db, { email: "not-an-email" })).rejects.toThrow(/email/);
  });
});

describe("the demo player in the app", () => {
  it("is recognised by its username, which no platform allows", () => {
    expect(isDemoUsername(DEMO_USERNAME)).toBe(true);
    expect(DEMO_USERNAME).not.toMatch(/^[A-Za-z0-9_-]+$/);
    expect(isDemoUsername("demo-player")).toBe(false);
  });

  it("marks the user's session as demo only while the demo is linked", async () => {
    await seedDemo(db, { email: EMAIL });
    const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, EMAIL));
    expect(await hasDemoAccount(db, u?.id as string)).toBe(true);
    await removeDemo(db);
    expect(await hasDemoAccount(db, u?.id as string)).toBe(false);
  });

  it("never asks a platform to sync the demo player", async () => {
    const { accountId } = await seedDemo(db, { email: EMAIL });
    const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, EMAIL));
    const out = await requestSync(db, u?.id as string, accountId);
    expect(out).toEqual({ kind: "ok", accounts: [] });
    const jobs = (await db.execute(
      sql`select count(*)::int as n from jobs where kind = 'import-account'`,
    )) as unknown as { n: number }[];
    expect(jobs[0]?.n).toBe(0);
  });
});
