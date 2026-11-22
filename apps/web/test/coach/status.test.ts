import { aiStatus } from "@ci/db/generated";
import { sql } from "drizzle-orm";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { coachStatus } from "../../src/server/coach/status.js";
import { resetAuthTables, webTestDb } from "../helpers/session";

// Bug hunt I5: the web offered "Build my report" and "Ask your first question" while no AI could
// write. The worker records in `ai_status` whether its AI runners have a client and keeps the row
// fresh (ADR 0078); the web reads only that.

const db = webTestDb();

beforeAll(async () => {
  await resetAuthTables(db);
});

beforeEach(async () => {
  await db.execute(sql`truncate table ai_status`);
});

const row = (enabled: boolean, ageSeconds: number) => ({
  workerId: crypto.randomUUID(),
  enabled,
  detail: enabled ? "ai: enabled (sidecar, batches off)" : "ai: disabled (no client)",
  heartbeatAt: sql`now() - make_interval(secs => ${ageSeconds})` as unknown as Date,
});

describe("coachStatus", () => {
  it("is offline when no worker has written a row", async () => {
    expect(await coachStatus(db)).toBe("offline");
  });

  it("is available while an enabled worker's heartbeat is fresh", async () => {
    await db.insert(aiStatus).values(row(true, 5));
    expect(await coachStatus(db)).toBe("available");
  });

  it("is no_ai while the running workers have no AI client", async () => {
    await db.insert(aiStatus).values(row(false, 5));
    expect(await coachStatus(db)).toBe("no_ai");
  });

  it("a stale heartbeat counts as no worker at all", async () => {
    await db.insert(aiStatus).values(row(true, 600));
    expect(await coachStatus(db)).toBe("offline");
  });

  it("one fresh enabled worker is enough beside a disabled one", async () => {
    await db.insert(aiStatus).values([row(false, 5), row(true, 5)]);
    expect(await coachStatus(db)).toBe("available");
  });
});
