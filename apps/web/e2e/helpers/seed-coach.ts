import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import {
  accountLinks,
  assertSafeTestDatabase,
  createDb,
  type Db,
  linkedAccounts,
  user,
} from "@ci/db";
import { gameMetrics } from "@ci/db/generated";
import { and, eq, sql } from "drizzle-orm";
import { seedThread } from "../../test/helpers/coach";
import { DISCLOSURE, seedNote, seedReport, seedWeekly } from "../../test/helpers/coaching";
import { seedProfile } from "../../test/helpers/profile";
import { E2E_ACCOUNT, E2E_USER, isE2eUsername, REFUSAL, seedE2eGames } from "./seed-games";

// The coach e2e seed (M5b Task 11): synthetic rows only, on the e2e-fixtures account of the adult
// e2e user, rapid. It reuses M5a's games seed (and its guard) and the Task 1, 2 and 5 test
// writers. Every call first clears the account's coaching rows and writes them again, so every
// spec and visual project sees the same world whatever ran before (a rebuild in coach.spec
// adds a report version; the next seed takes it away). Dates are fixed, so the pages read the
// same on every run: the complete report is from 13 Oct 2026, well outside the beta's one-day
// manual interval, so "Rebuild report" is allowed. Run as a script (tsx with
// `--conditions development`) it prints `COACH=<json>`; `no-report` seeds it without a report; `link <email>` links a second, already
// signed-in user to the same account (the privacy check).

export type E2eCoach = { accountId: string; threadId: string; streamingMessageId: string };

const DEFAULT_URL = "postgres://postgres:postgres@localhost:55432/chess_e2e_test";
const TC = "rapid" as const;
const INPUT = new URL(
  "../../../../backend/fixtures/ai/input/coaching-input.b2-rapid.json",
  import.meta.url,
);
/** Fixed game ids of the games seed (`idOf` there): its 30 synthetic games are 10 to 39. */
const idOf = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
/** The synthetic games seeded rapid (even i), oldest first, with their dates. */
const RAPID = Array.from({ length: 15 }, (_, j) => {
  const i = j * 2;
  return {
    id: idOf(10 + i),
    playedAt: new Date(Date.UTC(2026, 8, 1, 10 + (i % 3)) + i * 2 * 864e5),
  };
});
const at = (day: string) => new Date(`${day}T12:00:00Z`);

function dbOf(db?: Db): Db {
  return db ?? createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL ?? DEFAULT_URL));
}

async function refuseRealAccounts(db: Db): Promise<void> {
  const accounts = await db.select({ username: linkedAccounts.username }).from(linkedAccounts);
  if (accounts.some((a) => !isE2eUsername(a.username))) throw new Error(REFUSAL);
}

/** Clears what this seed writes for the account (the games stay). */
async function clear(db: Db, accountId: string): Promise<void> {
  await db.execute(sql`delete from coach_conversations where account_id = ${accountId}`);
  for (const table of [
    "weakness_estimates",
    "weakness_state_history",
    "weakness_series_point",
    "kpi_snapshot",
    "game_metrics",
    "weekly_reviews",
    "weakness_change_notes",
    "user_events",
  ]) {
    await db.execute(sql`delete from ${sql.identifier(table)} where account_id = ${accountId}`);
  }
  await db.execute(sql`
    delete from report_sections where report_id in (select id from reports where account_id = ${accountId})`);
  await db.execute(sql`update report_runs set report_id = null where account_id = ${accountId}`);
  await db.execute(sql`delete from reports where account_id = ${accountId}`);
  await db.execute(sql`delete from report_runs where account_id = ${accountId}`);
  // No worker runs in the e2e stack: the jobs the web enqueued (rebuilds, questions) are inert.
  await db.execute(sql`delete from jobs where kind in ('generate-report', 'coach-chat')`);
  // Sync now (Task P2) and its simulated imports: "Not synced yet." again.
  await db.execute(sql`delete from import_runs where account_id = ${accountId}`);
  await db.execute(sql`delete from jobs where kind = 'import-account' and user_key = ${accountId}`);
}

/** ISO week keys from the week of 31 Aug 2026 (2026-W36) to 2026-W47. */
const WEEKS = Array.from({ length: 12 }, (_, i) => `2026-W${36 + i}`);
/** The week with no analysed games: stored as a null value with n 0. */
const NULL_WEEK = "2026-W41";

async function profile(db: Db, accountId: string): Promise<void> {
  // The rapid games get their game_metrics rows (the coach scope counts them).
  for (const g of RAPID) {
    await db.insert(gameMetrics).values({
      gameId: g.id,
      accountId,
      timeClass: TC,
      playedAt: g.playedAt,
      protocol: "e2e-1",
      metrics: { version: 1 },
    });
  }
  const est = (
    seriesKey: string,
    state: string,
    priority: number,
    k: number,
    n: number,
    mean: number,
    lo: number,
    hi: number,
  ) => ({
    seriesKey,
    state: state as "active",
    priority,
    k,
    n,
    kUnweighted: k,
    nUnweighted: n,
    alpha: k + 1,
    beta: n - k + 1,
    mean,
    ciLow: lo,
    ciHigh: hi,
    bandMedian: mean * 0.7,
    bandP40: mean * 0.6,
    excess: mean * 0.3,
    confidence: 0.85,
  });
  const points = RAPID.flatMap((g, j) => [
    {
      seriesKey: "W3.shallow",
      gameId: g.id,
      playedAt: g.playedAt,
      events: j % 3 === 0 ? 2 : 1,
      opportunities: 8,
      epl: 0.4,
    },
    {
      seriesKey: "W1",
      gameId: g.id,
      playedAt: g.playedAt,
      events: j % 4 === 0 ? 1 : 0,
      opportunities: 30,
      epl: 0.1,
    },
  ]);
  const weekly = (metric: string, base: number, step: number, n: number) =>
    WEEKS.map((week, i) =>
      week === NULL_WEEK
        ? { metric, windowKey: week, value: null, n: 0 }
        : {
            metric,
            windowKey: week,
            value: base + step * i,
            ciLow: base + step * i - 2,
            ciHigh: base + step * i + 2,
            n,
          },
    );
  const area = (name: string, value: number | null, n: number) => ({
    metric: `area:${name}`,
    windowKey: "last50",
    value,
    ciLow: value === null ? null : value - 6,
    ciHigh: value === null ? null : value + 6,
    n,
  });
  await seedProfile(db, accountId, {
    timeClass: TC,
    estimates: [
      est("W3.shallow", "active", 9, 22, 120, 0.18, 0.15, 0.22),
      est("W5", "emerging", 7, 9, 40, 0.22, 0.15, 0.3),
      est("W1", "active", 6, 58, 2400, 0.024, 0.021, 0.028),
      est("W8.basics", "improving", 5, 6, 70, 0.09, 0.05, 0.14),
      est("W9a", "relapsed", 4, 7, 30, 0.23, 0.14, 0.33),
      est("W7.traps", "emerging", 3, 2, 12, 0.17, 0.07, 0.32),
      est("W4.missed", "resolved", 2, 3, 90, 0.035, 0.015, 0.07),
      est("W6", "not_an_issue", 1, 1, 50, 0.02, 0.005, 0.06),
    ],
    history: [
      {
        seriesKey: "W3.shallow",
        kind: "transition",
        toState: "emerging",
        atPlayedAt: at("2026-09-03"),
        atGameId: RAPID[1]?.id,
      },
      {
        seriesKey: "W3.shallow",
        kind: "transition",
        fromState: "emerging",
        toState: "active",
        atPlayedAt: at("2026-09-25"),
        atGameId: RAPID[6]?.id,
        rateBefore: 0.11,
        rateAfter: 0.18,
        evidence: { k: 22, n: 120, confidence: 0.85 },
      },
      {
        seriesKey: "W8.basics",
        kind: "transition",
        toState: "active",
        atPlayedAt: at("2026-09-05"),
        atGameId: RAPID[1]?.id,
      },
      {
        seriesKey: "W8.basics",
        kind: "transition",
        fromState: "active",
        toState: "improving",
        atPlayedAt: at("2026-10-15"),
        atGameId: RAPID[11]?.id,
        rateBefore: 0.14,
        rateAfter: 0.09,
        evidence: { k: 6, n: 70, confidence: 0.82 },
      },
      {
        seriesKey: "W1",
        kind: "transition",
        toState: "active",
        atPlayedAt: at("2026-09-09"),
        atGameId: RAPID[2]?.id,
      },
    ],
    points,
    kpis: [
      ...weekly("accuracy", 71, 0.5, 22),
      ...weekly("blunders_per_game", 1.4, -0.03, 22),
      { metric: "accuracy", windowKey: "last50", value: 74.2, ciLow: 72.8, ciHigh: 75.6, n: 40 },
      {
        metric: "blunders_per_game",
        windowKey: "last50",
        value: 1.1,
        ciLow: 0.8,
        ciHigh: 1.4,
        n: 40,
      },
      area("openings", 48, 15),
      area("middlegame", 55, 15),
      area("endgame", 41, 15),
      area("tactics", 37, 15),
      // Not enough games for a calculation score: stored null, never shown as 0.
      area("calculation", null, 6),
      area("time", 60, 15),
    ],
  });
}

/**
 * Seeds the coach world for the adult e2e user (see the module comment). Refuses, writing
 * nothing, a database whose linked accounts are not all e2e ones (M5a's guard and message).
 */
export async function seedE2eCoach(
  opts: { db?: Db; email?: string; report?: boolean } = {},
): Promise<E2eCoach> {
  const db = dbOf(opts.db);
  await refuseRealAccounts(db);
  const email = opts.email ?? E2E_USER;
  await seedE2eGames({ db, email });
  const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  const [account] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.username, E2E_ACCOUNT));
  if (!u || !account) throw new Error("the games seed left no e2e user or account");
  const accountId = account.id;
  await clear(db, accountId);
  await profile(db, accountId);
  // The simulated worker stands in for one with an AI client (ADR 0078), so the coach pages
  // offer questions and reports. Its heartbeat is a day ahead: the specs outlast the 2 min window.
  await db.execute(
    sql`insert into ai_status (worker_id, enabled, detail, heartbeat_at)
        values ('00000000-0000-4000-8000-00000000e2e0', true, 'ai: enabled (e2e simulated worker)', now() + interval '1 day')
        on conflict (worker_id) do update set enabled = true, heartbeat_at = excluded.heartbeat_at`,
  );

  // The first report (onboarding) and the complete one the page shows; its run holds the
  // synthetic coaching input the exemplar boards come from (ruling R2).
  // `report: false` leaves the scope with no report ("Build my report", report.spec).
  const input = JSON.parse(readFileSync(INPUT, "utf8")) as Record<string, unknown>;
  if (opts.report !== false) {
    await seedReport(db, accountId, {
      timeClass: TC,
      payload: "example",
      createdAt: at("2026-09-14"),
      run: { input: { ...input, accountId } },
    });
    await seedReport(db, accountId, {
      timeClass: TC,
      payload: "example",
      createdAt: at("2026-10-13"),
      run: { trigger: "manual", isFirstReport: false, input: { ...input, accountId } },
    });
  }
  await seedWeekly(db, accountId, { timeClass: TC, weekStart: "2026-11-09" });
  await seedWeekly(db, accountId, { timeClass: TC, weekStart: "2026-11-16", status: "pending" });
  await seedNote(db, accountId, {
    changeKey: "e2e-w3-onset",
    seriesKey: "W3.shallow",
    onsetDate: "2026-09-25",
  });

  const text = (t: string) => [{ kind: "text" as const, text: t }];
  const chat = { promptVersion: "chat@1", generatedAt: "2026-11-04T09:00:00.000Z" };
  const { threadId } = await seedThread(db, {
    userId: u.id,
    accountId,
    timeClass: TC,
    title: "Why do I miss short tactics?",
    lastMessageAt: at("2026-11-04"),
    messages: [
      {
        role: "user",
        status: "done",
        text: "Why do I miss short tactics?",
        segments: text("Why do I miss short tactics?"),
        createdAt: at("2026-11-04"),
      },
      {
        role: "assistant",
        status: "done",
        text: "Most of the misses come after a capture, when you recapture at once.",
        segments: text("Most of the misses come after a capture, when you recapture at once."),
        aiDisclosure: { ...DISCLOSURE, ...chat },
        createdAt: at("2026-11-04"),
      },
    ],
  });
  // An answer still streaming, in its own thread: the privacy check streams chunks to it.
  const { messageIds } = await seedThread(db, {
    userId: u.id,
    accountId,
    timeClass: TC,
    title: "How should I spend my clock?",
    lastMessageAt: at("2026-11-05"),
    messages: [
      {
        role: "user",
        status: "done",
        text: "How should I spend my clock?",
        segments: text("How should I spend my clock?"),
        createdAt: at("2026-11-05"),
      },
      { role: "assistant", status: "streaming", createdAt: at("2026-11-05") },
    ],
  });
  const streamingMessageId = messageIds[1];
  if (!streamingMessageId) throw new Error("no streaming message");
  // It streams on its first attempt, as the job reserves one when it starts.
  await db.execute(sql`update coach_messages set turn_attempts = 1, streaming_at = now()
    where id = ${streamingMessageId}`);
  return { accountId, threadId, streamingMessageId };
}

/** Links a second, already signed-in user to the e2e account (unverified, by username). */
export async function linkPeer(email: string, opts: { db?: Db } = {}): Promise<void> {
  const db = dbOf(opts.db);
  await refuseRealAccounts(db);
  const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  const [account] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.username, E2E_ACCOUNT));
  if (!u || !account) throw new Error(`no user ${email} or no ${E2E_ACCOUNT}; seed first`);
  const [have] = await db
    .select({ id: accountLinks.id })
    .from(accountLinks)
    .where(and(eq(accountLinks.userId, u.id), eq(accountLinks.linkedAccountId, account.id)));
  if (!have) {
    await db
      .insert(accountLinks)
      .values({ userId: u.id, linkedAccountId: account.id, method: "username" });
  }
}

/**
 * Uses `n` of this week's coach answers for the adult user, as another device would: one thread
 * of `n` done answers created now, each with its disclosure (they count, R6). For the cap check.
 */
export async function spendAllowance(n: number, opts: { db?: Db } = {}): Promise<void> {
  const db = dbOf(opts.db);
  await refuseRealAccounts(db);
  const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, E2E_USER));
  const [account] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(eq(linkedAccounts.username, E2E_ACCOUNT));
  if (!u || !account) throw new Error("seed the coach first");
  const { threadId } = await seedThread(db, {
    userId: u.id,
    accountId: account.id,
    timeClass: TC,
    title: "Earlier questions",
  });
  await db.execute(sql`
    insert into coach_messages (conversation_id, seq, role, status, text, segments, ai_disclosure)
    select ${threadId}, g, 'assistant', 'done', 'An earlier answer.',
      '[{"kind":"text","text":"An earlier answer."}]'::jsonb,
      ${JSON.stringify({ ...DISCLOSURE, promptVersion: "chat@1" })}::jsonb
    from generate_series(1, ${n}) as g`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [command, arg] = process.argv.slice(2);
  if (command === "link" && arg) {
    await linkPeer(arg);
    console.log("LINKED");
  } else if (command === "no-report") {
    console.log(`COACH=${JSON.stringify(await seedE2eCoach({ report: false }))}`);
  } else if (command === "spend" && arg) {
    await spendAllowance(Number(arg));
    console.log("SPENT");
  } else {
    console.log(`COACH=${JSON.stringify(await seedE2eCoach(command ? { email: command } : {}))}`);
  }
  process.exit(0);
}
