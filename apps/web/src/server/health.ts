import { createDb } from "@ci/db";
import { sql } from "drizzle-orm";
import { Redis } from "ioredis";
import { getEnv } from "../env";

export type Health = { ok: boolean; db: boolean; redis: boolean };

let db: ReturnType<typeof createDb> | undefined;
let redis: Redis | undefined;

async function checkDb(): Promise<boolean> {
  try {
    db ??= createDb(getEnv().DATABASE_URL);
    await db.execute(sql`select 1`);
    return true;
  } catch {
    return false;
  }
}

async function checkRedis(): Promise<boolean> {
  try {
    redis ??= new Redis(getEnv().REDIS_URL, {
      lazyConnect: true,
      maxRetriesPerRequest: 1,
      connectTimeout: 2000,
    });
    if (redis.status === "wait" || redis.status === "end") await redis.connect();
    return (await redis.ping()) === "PONG";
  } catch {
    return false;
  }
}

export async function checkHealth(): Promise<Health> {
  const [dbOk, redisOk] = await Promise.all([checkDb(), checkRedis()]);
  return { ok: dbOk && redisOk, db: dbOk, redis: redisOk };
}
