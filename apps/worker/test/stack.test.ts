import { createConnection } from "node:net";
import postgres from "postgres";
import { describe, expect, it } from "vitest";

const inDocker = process.env.IN_DOCKER === "1";

function redisPing(url: string): Promise<string> {
  const { hostname, port } = new URL(url);
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host: hostname, port: Number(port || 6379) });
    socket.setTimeout(5000, () => {
      socket.destroy();
      reject(new Error("redis PING timed out"));
    });
    socket.on("error", reject);
    socket.on("connect", () => socket.write("PING\r\n"));
    socket.on("data", (data) => {
      socket.end();
      resolve(data.toString().trim());
    });
  });
}

// Inside the container, unreachable services must fail the suite; outside it, skip.
describe.skipIf(!inDocker)("compose stack connectivity", () => {
  it("reaches redis at REDIS_URL", async () => {
    const url = process.env.REDIS_URL;
    expect(url, "REDIS_URL must be set").toBeTruthy();
    expect(await redisPing(url as string)).toBe("+PONG");
  });

  it("reaches postgres at DATABASE_URL_TEST", async () => {
    const url = process.env.DATABASE_URL_TEST;
    expect(url, "DATABASE_URL_TEST must be set").toBeTruthy();
    const sql = postgres(url as string, { connect_timeout: 5 });
    try {
      const rows = await sql`select 1 as one`;
      expect(rows[0]?.one).toBe(1);
    } finally {
      await sql.end();
    }
  });
});
