import { accountLinks, jobs } from "@ci/db/generated";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { frameReader } from "../events/helpers";
import { resetCoachTables, seedChunk, seedThread } from "../helpers/coach";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
const threads = await import("../../src/app/api/coach/threads/route");
const thread = await import("../../src/app/api/coach/threads/[threadId]/route");
const messages = await import("../../src/app/api/coach/threads/[threadId]/messages/route");
const events = await import("../../src/app/api/events/route");
const { getEventHub } = await import("../../src/server/events/hub");

const ORIGIN = "http://localhost:3100";

type Who = { id: string; cookie: string };
let a: Who;
let b: Who;
let accountId: string;
const open: AbortController[] = [];

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachTables(db);
  const sa = await createTestSession(db);
  const sb = await createTestSession(db);
  a = { id: sa.user.id, cookie: sa.cookie };
  b = { id: sb.user.id, cookie: sb.cookie };
  accountId = (await seedAccount(db, { userId: a.id, username: "shared" })).accountId;
  await db
    .insert(accountLinks)
    .values({ userId: b.id, linkedAccountId: accountId, method: "username" });
});

afterEach(async () => {
  for (const c of open.splice(0)) c.abort();
  await expect.poll(() => getEventHub().channelCount()).toBe(0);
});

function req(
  path: string,
  opts: {
    method?: string;
    body?: unknown;
    raw?: string;
    cookie?: string | null;
    origin?: string | null;
  } = {},
) {
  const headers = new Headers();
  const cookie = opts.cookie === undefined ? a.cookie : opts.cookie;
  if (cookie) headers.set("cookie", cookie);
  const method = opts.method ?? "GET";
  const origin = opts.origin === undefined ? (method === "GET" ? null : ORIGIN) : opts.origin;
  if (origin) headers.set("origin", origin);
  if (method !== "GET") headers.set("content-type", "application/json");
  const body = opts.raw ?? (opts.body === undefined ? undefined : JSON.stringify(opts.body));
  return new Request(`${ORIGIN}${path}`, {
    method,
    headers,
    ...(body === undefined ? {} : { body }),
  });
}

const params = (threadId: string) => ({ params: Promise.resolve({ threadId }) });

async function create(body: object, cookie?: string) {
  return threads.POST(
    req("/api/coach/threads", { method: "POST", body, ...(cookie ? { cookie } : {}) }),
  );
}

describe("coach thread routes", () => {
  it("signed-out is 401", async () => {
    expect(
      (await threads.GET(req(`/api/coach/threads?account=${accountId}`, { cookie: null }))).status,
    ).toBe(401);
    expect((await create({ accountId, timeClass: "rapid" }, "ci.session_token=nope")).status).toBe(
      401,
    );
  });

  it("cross-site POST is 403", async () => {
    const res = await threads.POST(
      req("/api/coach/threads", {
        method: "POST",
        body: { accountId, timeClass: "rapid" },
        origin: "https://evil.example",
      }),
    );
    expect(res.status).toBe(403);
    const { threadId } = await seedThread(db, { userId: a.id, accountId, timeClass: "rapid" });
    const post = await messages.POST(
      req(`/api/coach/threads/${threadId}/messages`, {
        method: "POST",
        body: { question: "Hi" },
        origin: null,
      }),
      params(threadId),
    );
    expect(post.status).toBe(403);
    const del = await thread.DELETE(
      req(`/api/coach/threads/${threadId}`, { method: "DELETE", origin: "https://evil.example" }),
      params(threadId),
    );
    expect(del.status).toBe(403);
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("201 on create with a first question", async () => {
    const res = await create({
      accountId,
      timeClass: "rapid",
      anchor: { kind: "weakness", weaknessId: "W3" },
      question: "How do I fix this?",
    });
    expect(res.status).toBe(201);
    const body = (await res.json()) as { id: string; assistantMessageId: string };
    expect(body.assistantMessageId).toMatch(/^[0-9a-f-]{36}$/);
    const got = await thread.GET(req(`/api/coach/threads/${body.id}`), params(body.id));
    expect(got.status).toBe(200);
    const t = (await got.json()) as { title: string; messages: { role: string; status: string }[] };
    expect(t.title).toBe("Chat about a weakness");
    expect(t.messages.map((m) => [m.role, m.status])).toEqual([
      ["user", "done"],
      ["assistant", "pending"],
    ]);
    const [job] = await db.select().from(jobs);
    expect(job?.payload).toEqual({ messageId: body.assistantMessageId });

    const list = await threads.GET(req(`/api/coach/threads?account=${accountId}`));
    expect(((await list.json()) as { id: string }[]).map((x) => x.id)).toEqual([body.id]);
  });

  it("create without a question, and validation", async () => {
    const res = await create({ accountId, timeClass: "blitz" });
    expect(res.status).toBe(201);
    expect(await res.json()).toEqual({ id: expect.any(String) });
    expect((await create({ accountId, timeClass: "hyper" })).status).toBe(400);
    expect((await create({ accountId: "nope", timeClass: "rapid" })).status).toBe(404);
    const foreign = (await seedAccount(db, { username: "stranger" })).accountId;
    expect((await create({ accountId: foreign, timeClass: "rapid" })).status).toBe(404);
    expect(
      (await create({ accountId, timeClass: "rapid", question: "x".repeat(2001) })).status,
    ).toBe(400);
    expect((await create({ accountId, timeClass: "rapid", question: "   " })).status).toBe(400);
    // A malformed anchor is dropped, not an error.
    const dropped = await create({ accountId, timeClass: "rapid", anchor: { kind: "weakness" } });
    expect(dropped.status).toBe(201);
    const raw = await threads.POST(req("/api/coach/threads", { method: "POST", raw: "{oops" }));
    expect(raw.status).toBe(400);
    const huge = await threads.POST(
      req("/api/coach/threads", {
        method: "POST",
        raw: JSON.stringify({ accountId, timeClass: "rapid", pad: "x".repeat(40_000) }),
      }),
    );
    expect(huge.status).toBe(413);
  });

  it("409 on busy, 400 on empty or too long, 202 when posted", async () => {
    const { threadId } = await seedThread(db, { userId: a.id, accountId, timeClass: "rapid" });
    const post = (body: object) =>
      messages.POST(
        req(`/api/coach/threads/${threadId}/messages`, { method: "POST", body }),
        params(threadId),
      );
    const first = await post({ question: "First" });
    expect(first.status).toBe(202);
    expect(await first.json()).toEqual({
      userMessageId: expect.any(String),
      assistantMessageId: expect.any(String),
    });
    const busy = await post({ question: "Second" });
    expect(busy.status).toBe(409);
    expect(await busy.json()).toEqual({ error: "busy" });
    expect(await (await post({ question: "" })).json()).toEqual({ error: "empty" });
    const long = await post({ question: "x".repeat(2001) });
    expect(long.status).toBe(400);
    expect(await long.json()).toEqual({ error: "too_long" });
    expect((await post({ question: 7 })).status).toBe(400);
  });

  it("404 on a foreign thread, for read, post and delete", async () => {
    const { threadId } = await seedThread(db, { userId: a.id, accountId, timeClass: "rapid" });
    const asB = { cookie: b.cookie };
    expect(
      (await thread.GET(req(`/api/coach/threads/${threadId}`, asB), params(threadId))).status,
    ).toBe(404);
    const post = await messages.POST(
      req(`/api/coach/threads/${threadId}/messages`, {
        method: "POST",
        body: { question: "Hi" },
        ...asB,
      }),
      params(threadId),
    );
    expect(post.status).toBe(404);
    const del = await thread.DELETE(
      req(`/api/coach/threads/${threadId}`, { method: "DELETE", ...asB }),
      params(threadId),
    );
    expect(del.status).toBe(404);
    expect((await thread.GET(req("/api/coach/threads/garbage"), params("garbage"))).status).toBe(
      404,
    );
    const own = await thread.DELETE(
      req(`/api/coach/threads/${threadId}`, { method: "DELETE" }),
      params(threadId),
    );
    expect(own.status).toBe(204);
  });

  it("403 not_linked when posting after unlinking", async () => {
    const { threadId } = await seedThread(db, { userId: b.id, accountId, timeClass: "rapid" });
    const { eq } = await import("drizzle-orm");
    await db.delete(accountLinks).where(eq(accountLinks.userId, b.id));
    const res = await messages.POST(
      req(`/api/coach/threads/${threadId}/messages`, {
        method: "POST",
        body: { question: "Hi" },
        cookie: b.cookie,
      }),
      params(threadId),
    );
    expect(res.status).toBe(403);
    expect(await res.json()).toEqual({ error: "not_linked" });
  });

  it("user 2 never receives user 1's coach events", async () => {
    const res = await create({ accountId, timeClass: "rapid", question: "Mine" });
    const { id, assistantMessageId } = (await res.json()) as {
      id: string;
      assistantMessageId: string;
    };

    const listen = (cookie: string) => {
      const controller = new AbortController();
      open.push(controller);
      const headers = new Headers({ cookie, "last-event-id": "0" });
      return events.GET(
        new Request(`${ORIGIN}/api/events`, { headers, signal: controller.signal }),
      );
    };
    const mine = frameReader(await listen(a.cookie));
    const theirs = frameReader(await listen(b.cookie));

    // The worker's part: a chunk and the ready event, owned by user 1.
    const chunk = await seedChunk(db, {
      userId: a.id,
      accountId,
      messageId: assistantMessageId,
      seq: 1,
      segments: [{ kind: "text", text: "secret" }],
    });
    const { insertUserEvent } = await import("@ci/db");
    const ready = await insertUserEvent(db, {
      accountId,
      userId: a.id,
      type: "chat.message.ready",
      payload: { messageId: assistantMessageId, replace: false },
    });
    const marker = await insertUserEvent(db, {
      accountId,
      type: "import.progress",
      payload: { imported: 1 },
    });

    const seen = async (frames: ReturnType<typeof frameReader>) => {
      const ids: string[] = [];
      for (;;) {
        const f = await frames.nextEvent();
        if (!f?.id) break;
        ids.push(f.id);
        if (f.id === String(marker)) break;
      }
      return ids;
    };
    expect(await seen(mine)).toEqual([String(chunk), String(ready), String(marker)]);
    expect(await seen(theirs)).toEqual([String(marker)]);

    // And user 2's reload of the thread is not found, so no stored chunk replays either.
    const reload = await thread.GET(
      req(`/api/coach/threads/${id}`, { cookie: b.cookie }),
      params(id),
    );
    expect(reload.status).toBe(404);
    expect(await reload.json()).toEqual({ error: "not_found" });
  });
});
