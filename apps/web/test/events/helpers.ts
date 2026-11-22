import { randomUUID } from "node:crypto";
import { accountLinks, type Db, insertUserEvent, linkedAccounts } from "@ci/db";
import { sql } from "drizzle-orm";

/**
 * Records an event the way the publishers do (Rust `events::store::publish`, mirrored by
 * `@ci/db` `insertUserEvent`): advisory lock, insert, `NOTIFY user_events` on commit.
 */
export function insertAndPublish(
  db: Db,
  accountId: string,
  type: string,
  payload: object = {},
): Promise<number> {
  return insertUserEvent(db, { accountId, type, payload });
}

/** Rust `prune_events` for a fixed id: deletes `id <= through` and raises the watermark. */
export async function pruneThrough(db: Db, through: number): Promise<void> {
  await db.execute(sql`
    with d as (delete from user_events where id <= ${through} returning id)
    update user_events_watermark
       set pruned_through_id = greatest(pruned_through_id, coalesce((select max(id) from d), 0))`);
}

/** Empties the event and account tables and resets the watermark (ids restart at 1). */
export async function resetEventTables(db: Db): Promise<void> {
  await db.execute(sql`truncate table user_events, linked_accounts restart identity cascade`);
  await db.execute(sql`update user_events_watermark set pruned_through_id = 0`);
}

/** A chess.com account linked (unverified unless `verified`) to the user; returns its id. */
export async function linkAccount(db: Db, userId: string, verified = false): Promise<string> {
  const username = `player-${randomUUID().slice(0, 8)}`;
  const [account] = await db
    .insert(linkedAccounts)
    .values({ platform: "chesscom", username, displayName: username })
    .returning();
  if (!account) throw new Error("linkAccount: no row");
  await db.insert(accountLinks).values({
    userId,
    linkedAccountId: account.id,
    method: "location_code",
    verifiedAt: verified ? new Date() : null,
  });
  return account.id;
}

export type Frame = {
  id?: string;
  event?: string;
  data?: string;
  comment?: string;
  retry?: string;
};

/** Parses one SSE frame (the text between blank lines). */
export function parseFrame(text: string): Frame {
  const frame: Frame = {};
  for (const line of text.split("\n")) {
    if (line.startsWith(":")) frame.comment = line.slice(1).trim();
    const match = /^(id|event|data|retry): ?(.*)$/.exec(line);
    if (match) frame[match[1] as "id" | "event" | "data" | "retry"] = match[2];
  }
  return frame;
}

/** Reads SSE frames from a response body. `next` resolves null when the stream ends. */
export function frameReader(res: Response) {
  if (!res.body) throw new Error("frameReader: no body");
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let pending = "";
  const ready: Frame[] = [];
  let done = false;

  async function fill(): Promise<void> {
    while (ready.length === 0 && !done) {
      const chunk = await reader.read();
      if (chunk.done) {
        done = true;
        return;
      }
      pending += decoder.decode(chunk.value, { stream: true });
      let cut = pending.indexOf("\n\n");
      while (cut >= 0) {
        ready.push(parseFrame(pending.slice(0, cut)));
        pending = pending.slice(cut + 2);
        cut = pending.indexOf("\n\n");
      }
    }
  }

  return {
    /** The next frame, or null at the end of the stream. */
    async next(): Promise<Frame | null> {
      await fill();
      return ready.shift() ?? null;
    },
    /** The next frame within `ms`, or "timeout". */
    async nextWithin(ms: number): Promise<Frame | null | "timeout"> {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const timeout = new Promise<"timeout">((resolve) => {
        timer = setTimeout(() => resolve("timeout"), ms);
      });
      try {
        return await Promise.race([this.next(), timeout]);
      } finally {
        clearTimeout(timer);
      }
    },
    /** Next frame that carries an event (skips `retry:` and `: ping` frames). */
    async nextEvent(): Promise<Frame | null> {
      for (;;) {
        const frame = await this.next();
        if (frame === null || frame.event) return frame;
      }
    },
    cancel: () => reader.cancel(),
  };
}
