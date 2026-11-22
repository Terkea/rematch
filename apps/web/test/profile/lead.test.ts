import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { leadFinding } from "../../src/server/profile";
import { resetGameTables, seedAccount } from "../helpers/games";
import { seedProfile } from "../helpers/profile";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let userId: string;
let accountId: string;

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  userId = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId, username: "Hero" })).accountId;
});

describe("leadFinding", () => {
  it("lead is the top sufficient active weakness", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 4,
      estimates: [
        // Higher priority, but too few chances (W1 needs 1000).
        { seriesKey: "W1", priority: 9, state: "active", nUnweighted: 300 },
        { seriesKey: "W2", priority: 5, state: "resolved", nUnweighted: 5000 },
        { seriesKey: "W5", priority: 3, state: "emerging", nUnweighted: 40 },
        { seriesKey: "W6", priority: 2, state: "relapsed", nUnweighted: 40 },
      ],
    });
    const lead = await leadFinding(db, userId);
    expect(lead.kind).toBe("weakness");
    if (lead.kind !== "weakness") return;
    expect(lead.weakness.seriesKey).toBe("W5");
    expect(lead.scope).toMatchObject({ accountId, timeClass: "rapid" });
  });

  // ADR 0084 (bug hunt N10): a costing state the comparison does not back (confidence < 0.7,
  // the owner's rapid "Opening traps" at 0.547) is never "What to work on".
  it("a costing state below confidence 0.7 is not a lead", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 4,
      estimates: [
        { seriesKey: "W7.traps", priority: 9, state: "active", nUnweighted: 79, confidence: 0.547 },
        {
          seriesKey: "W4.allowed",
          priority: 1,
          state: "active",
          nUnweighted: 1599,
          confidence: 0.81,
        },
      ],
    });
    const lead = await leadFinding(db, userId);
    expect(lead.kind).toBe("weakness");
    if (lead.kind !== "weakness") return;
    expect(lead.weakness.seriesKey).toBe("W4.allowed");
  });

  it("resolved or insufficient weaknesses are not a lead", async () => {
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 7,
      estimates: [
        { seriesKey: "W1", priority: 9, state: "active", nUnweighted: 300 },
        { seriesKey: "W2", priority: 5, state: "resolved", nUnweighted: 5000 },
        { seriesKey: "W5", priority: 3, state: "improving", nUnweighted: 40 },
        { seriesKey: "W6", priority: 0, state: "active", nUnweighted: 40 },
      ],
    });
    const lead = await leadFinding(db, userId);
    expect(lead).toMatchObject({ kind: "none", analysedGames: 7, compared: true });
  });

  it("with no benchmark the lead is none and says so", async () => {
    const none = { state: "not_an_issue" as const, bandMedian: null, excess: null, priority: 0 };
    await seedProfile(db, accountId, {
      timeClass: "rapid",
      games: 7,
      estimates: [
        { seriesKey: "W1", ...none },
        { seriesKey: "W5", ...none },
      ],
    });
    expect(await leadFinding(db, userId)).toMatchObject({ kind: "none", compared: false });
  });

  it("no scope gives building", async () => {
    expect(await leadFinding(db, userId)).toEqual({ kind: "building" });
  });
});
