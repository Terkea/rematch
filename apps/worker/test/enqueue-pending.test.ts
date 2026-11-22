import { randomUUID } from "node:crypto";
import { type Db, games } from "@ci/db";
import { eq } from "drizzle-orm";
import type { Redis } from "ioredis";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { enqueuePending } from "../scripts/enqueue-pending.js";
import { createQueues, createRedisConnection, type Queues } from "../src/queue/queues.js";
import { getWorkerTestDb, workerTestDbConfigured } from "./db.js";
import { insertAccount, SCHOLARS } from "./jobs/helpers.js";

const url = process.env.REDIS_URL;

// Parity tooling (R1 Task 24); deleted with the TS worker in Task 27.
describe.skipIf(!workerTestDbConfigured() || (!url && process.env.IN_DOCKER !== "1"))(
  "enqueue-pending",
  () => {
    let db: Db;
    let connection: Redis;
    let queues: Queues;

    beforeAll(async () => {
      db = (await getWorkerTestDb()) as Db;
      connection = createRedisConnection(url as string);
      queues = createQueues(connection, { prefix: `test-${randomUUID()}` });
    });

    afterAll(async () => {
      await queues.obliterate();
      await queues.close();
      connection.disconnect();
    });

    it("enqueues an analyse job for every pending game, once", async () => {
      const acct = await insertAccount(db);
      const row = (id: string, status: "pending" | "done") => ({
        accountId: acct.id,
        platform: "lichess" as const,
        platformGameId: id,
        pgn: SCHOLARS,
        playedAt: new Date("2026-01-01T00:00:00Z"),
        timeClass: "blitz" as const,
        rated: false,
        playerColor: "white" as const,
        whiteName: "A",
        blackName: "B",
        result: "1-0" as const,
        analysisStatus: status,
      });
      const inserted = await db
        .insert(games)
        .values([row("p1", "pending"), row("p2", "pending"), row("d1", "done")])
        .returning({ id: games.id, platformGameId: games.platformGameId });

      expect(await enqueuePending(db, queues)).toBe(2);
      expect(await enqueuePending(db, queues)).toBe(0);

      for (const g of inserted) {
        const state = await queues.analyseJobState(g.id);
        expect([g.platformGameId, state]).toEqual([
          g.platformGameId,
          g.platformGameId === "d1" ? "unknown" : "waiting",
        ]);
      }
      const [p1] = await db.select().from(games).where(eq(games.platformGameId, "p1"));
      expect(p1?.analysisStatus).toBe("pending");
    });
  },
);
