import { assertSafeTestDatabase, createDb, user } from "@ci/db";
import { eq } from "drizzle-orm";
import { seedAccount, seedSyntheticGame } from "../../test/helpers/games";

// Run by filters.spec.ts (through tsx). Links the signed-in e2e user to a synthetic chess.com
// account and seeds synthetic rapid games (never real data) on fixed days, so a custom comparison
// always reads the same: September 2026 holds 20 games at a lower accuracy and more blunders,
// October 2026 20 games at a higher accuracy and fewer, and 1-5 November 2026 only 4 games, too
// few for period B. Both colours, every result, two formats. The seed helpers refuse anything but
// a `*_test` database.

const email = process.argv[2];
if (!email) throw new Error("usage: seed-filters.ts <email>");
const db = createDb(assertSafeTestDatabase(process.env.E2E_DATABASE_URL));
const [u] = await db.select({ id: user.id }).from(user).where(eq(user.email, email));
if (!u) throw new Error(`no e2e user ${email}`);

const { accountId } = await seedAccount(db, {
  userId: u.id,
  username: "e2e-filters",
  verified: true,
});
const results = ["1-0", "0-1", "1/2-1/2", "1-0"] as const;
const id = (n: number) => `00000000-0000-4000-8f17-${String(n).padStart(12, "0")}`;
let n = 0;
async function game(day: string, i: number, accuracy: number, blunders: number) {
  n += 1;
  await seedSyntheticGame(db, accountId, {
    id: id(n),
    playedAt: `${day}T${String(9 + (i % 10)).padStart(2, "0")}:00:00Z`,
    timeClass: "rapid",
    result: results[i % 4] ?? "1-0",
    playerColor: i % 2 === 0 ? "white" : "black",
    accuracy,
    counts: { blunders, mistakes: i % 2, inaccuracies: 1 + (i % 3) },
    baseSeconds: i % 3 === 0 ? 900 : 600,
    incrementSeconds: i % 3 === 0 ? 10 : 0,
    opponentRating: 1100 + ((i * 37) % 300),
  });
}
for (let i = 0; i < 20; i++) {
  await game(`2026-09-${String(1 + i).padStart(2, "0")}`, i, 55 + (i % 5), 2 + (i % 2));
}
for (let i = 0; i < 20; i++) {
  await game(`2026-10-${String(1 + i).padStart(2, "0")}`, i, 74 + (i % 5), i % 2);
}
for (let i = 0; i < 4; i++) {
  await game(`2026-11-0${1 + i}`, i, 70, 1);
}
console.log(`seeded ${n} filter games for ${email}`);
process.exit(0);
