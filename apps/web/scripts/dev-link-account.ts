import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { accountLinks, createDb, type Db, games, linkedAccounts, user } from "@ci/db";
import { and, eq, sql } from "drizzle-orm";
import postgres from "postgres";

// Local development only (ADR 0056): links an account the worker already imported to a user who
// has signed in once, as an UNVERIFIED `username` link, so the owner can open their games before
// the location-code flow (M4 T19/T21) is done. It never imports, and writes only `account_links`.
//
//   pnpm --filter @ci/web dev:link --email <e> --platform chesscom --username <name>

export type DevLinkInput = {
  email: string;
  platform: "chesscom" | "lichess";
  username: string;
};

export type DevLinkEnv = { NODE_ENV?: string; DATABASE_URL: string };

const LOCAL_HOSTS = new Set(["postgres", "localhost", "127.0.0.1"]);
const LOCAL_ONLY = "dev:link is for local development only";

/**
 * Throws unless this is a non-production run against a database on the machine (or compose).
 * The host is judged as postgres.js will connect (review I1): it honours `?host=`, splits the
 * userinfo at the last `@` and does not end the authority at `#`, unlike WHATWG `URL`. So any URL
 * those two could read differently is refused outright, and every host the driver would try must
 * be exactly one of `LOCAL_HOSTS`.
 */
export function assertLocal(env: DevLinkEnv): void {
  if (env.NODE_ENV === "production") throw new Error(LOCAL_ONLY);
  const url = env.DATABASE_URL;
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error(LOCAL_ONLY);
  }
  if (
    url.includes("#") ||
    url.split("@").length > 2 ||
    parsed.searchParams.has("host") ||
    parsed.searchParams.has("hostaddr") ||
    !LOCAL_HOSTS.has(parsed.hostname)
  ) {
    throw new Error(LOCAL_ONLY);
  }
  // Ask the driver itself. Building the client parses the URL and opens nothing.
  const client = postgres(url, { max: 1 });
  const targets: readonly string[] = client.options.host;
  void client.end({ timeout: 0 });
  if (targets.length === 0 || !targets.every((host) => LOCAL_HOSTS.has(host))) {
    throw new Error(LOCAL_ONLY);
  }
}

const IPV4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;

/**
 * Whether the server's own address (`inet_server_addr()`) is local: null (a unix socket),
 * loopback, or a private network such as the compose bridge. Anything public is remote.
 */
export function isLocalServerAddress(address: string | null): boolean {
  if (address === null) return true;
  const addr = address.toLowerCase().replace(/^::ffff:/, "");
  const v4 = IPV4.exec(addr);
  if (v4) {
    const [a, b] = [Number(v4[1]), Number(v4[2])];
    return a === 127 || a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168);
  }
  return addr === "::1" || /^f[cd][0-9a-f]{0,2}:/.test(addr);
}

/** After connecting: refuses unless the server we reached is on this machine or network. */
async function assertLocalServer(db: Db): Promise<void> {
  const [row] = (await db.execute(sql`select host(inet_server_addr()) as address`)) as unknown as {
    address: string | null;
  }[];
  if (!isLocalServerAddress(row?.address ?? null)) throw new Error(LOCAL_ONLY);
}

export async function devLinkAccount(
  db: Db,
  input: DevLinkInput,
  env: DevLinkEnv,
): Promise<{ linked: boolean; games: number }> {
  assertLocal(env);
  await assertLocalServer(db);
  const email = input.email.trim().toLowerCase();
  const username = input.username.trim().toLowerCase();

  const [found] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
  if (!found) throw new Error("No user with that email. Sign in once first.");

  const [account] = await db
    .select({ id: linkedAccounts.id })
    .from(linkedAccounts)
    .where(and(eq(linkedAccounts.platform, input.platform), eq(linkedAccounts.username, username)));
  if (!account) {
    throw new Error(
      `No imported account ${input.platform}/${username}. Import it with the worker first.`,
    );
  }

  const inserted = await db
    .insert(accountLinks)
    .values({ userId: found.id, linkedAccountId: account.id, method: "username", verifiedAt: null })
    .onConflictDoNothing()
    .returning({ id: accountLinks.id });

  const [count] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(games)
    .where(eq(games.accountId, account.id));
  return { linked: inserted.length > 0, games: count?.n ?? 0 };
}

export function linkedMessage(platform: string, username: string, count: number): string {
  return `Linked ${platform}/${username} (${count} ${count === 1 ? "game" : "games"}) to your account.`;
}

async function main(): Promise<void> {
  const { values } = parseArgs({
    options: {
      email: { type: "string" },
      platform: { type: "string", default: "chesscom" },
      username: { type: "string" },
    },
  });
  const { email, platform, username } = values;
  if (!email || !username || (platform !== "chesscom" && platform !== "lichess")) {
    throw new Error(
      "Usage: pnpm --filter @ci/web dev:link --email <e> --platform chesscom --username <name>",
    );
  }
  const env: DevLinkEnv = {
    NODE_ENV: process.env.NODE_ENV,
    DATABASE_URL: process.env.DATABASE_URL ?? "",
  };
  // Guard before connecting, so a remote DATABASE_URL is never even opened.
  assertLocal(env);
  const db = createDb(env.DATABASE_URL);
  try {
    const result = await devLinkAccount(db, { email, platform, username }, env);
    const name = username.toLowerCase();
    console.log(
      result.linked
        ? linkedMessage(platform, name, result.games)
        : `${platform}/${name} (${result.games} games) was already linked to your account.`,
    );
  } finally {
    await db.$client.end();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
