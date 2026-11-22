# ADR-WEB-8. Live user events: one LISTEN per process, SSE with replay, polling fallback

Date: 2026-11-21. Status: accepted. Implements M4 Task 22. Consumes ADR 0034 (user events over LISTEN/NOTIFY with a reset marker); builds on ADR-WEB-3 (account links) and ADR 0041 (private beta, one web process).

## Context

The web app shows import, analysis and report progress live. ADR 0034 fixes the publisher side: the Rust `events` crate inserts into `user_events` under advisory lock `7262011` and sends `NOTIFY user_events` with the camelCase body `{id, accountId, type, createdAt, payload}` (or `truncated: true` without `payload` above 7900 bytes) in the same transaction. Pruned events raise `user_events_watermark.pruned_through_id`, and a cursor below it must be told to refetch. The plan's Task 22 brief was written for the M1 Redis fan-out (`user-events:<accountId>`, one subscriber per process); Redis delivery goes away with the Rust worker, so the web consumes the Postgres channel instead. Task 2's `userEventsChannel`, `StreamEvent` and `oldestEventId` were never built; they are built here with Task 2's values.

## Decision

### Publishing from TypeScript matches Rust

`@ci/db` `insertUserEvent` (the TS worker's writer) now also calls `pg_notify('user_events', body)` inside its transaction, with the same fields as Rust (same names, `createdAt` with six fractional digits from the row's own `created_at`, the same 7900-byte truncation rule; key order may differ because Rust's serde_json sorts keys, and consumers parse the JSON). Both publishers therefore reach the same consumer, and a rolled-back insert notifies nobody. The TS worker's Redis `PUBLISH` stays untouched until it is retired; nothing in the web reads it. `userEventsChannel` (`user-events:<id>`) moved into `@ci/db` with Task 2's value and is still used only by that worker.

### One hub per process (`server/events/hub.ts`)

- `getEventHub()` holds one postgres.js `listen("user_events")` connection for the whole process (the app pool's client, which keeps LISTEN on its own dedicated connection). LISTEN starts lazily with the first subscriber and stays up.
- All accounts share the channel, so the hub routes each notification **only to subscribers of its `accountId`**; a subscriber registers with its own account ids, and nothing else is ever handed to it. `channelCount()` is the number of accounts with at least one subscriber (refcounted).
- Notifications are handled strictly one at a time. A truncated one is completed by reading the row by id before the next is handled, so id order holds; a truncated notification for an account nobody watches is not fetched.
- Malformed bodies and event types that are not dotted lower-case names (`^[a-z][a-z0-9_.-]{0,63}$`, so no SSE field can be injected) are skipped.
- postgres.js rebuilds a lost LISTEN connection and calls `onlisten` again. Every call after the first is ADR 0034's `Reconnected`: each subscriber starts buffering at once and replays from its last sent id.

### `GET /api/events`

Both modes first apply `rejectCrossSite` (a foreign `Origin` is 403) and `requireApiUser` (401 without a session, 403 `age_required` before the age screen, ADR 0041's invite-only gate applies at sign-in). The user's accounts are `userAccountIds`: every `account_links` row of the user, verified or not, as the brief requires. That is the only filter on what is read, so another user's events can be neither replayed nor received live. An unverified link sees that platform account's events, which describe public games; this follows the data model of ADR-WEB-3, where two users may hold unverified links to one account.

Cursors (`Last-Event-ID`, `?since=`, `?after=`) must be 1–15 digits, otherwise 400 `invalid_cursor`.

**SSE mode** (`text/event-stream; charset=utf-8`, `Cache-Control: no-cache, no-transform`, `X-Accel-Buffering: no`, opens with `retry: 3000`), following ADR 0034's consumer protocol:

1. Subscribe and buffer live events, and wait until LISTEN is active (503 if it cannot be).
2. With a cursor, replay `listEventsPage(accounts, lastId, 500)` page by page. On `reset` (cursor `< pruned_through_id`), send `id: <head>\nevent: reset\ndata: {}` and continue from the head. Without a cursor, send `id: <head>\nevent: hello\ndata: {}`.
3. Flush the buffer, then go live; every id `<=` the last id sent is dropped. Exactly-once is exact because NOTIFYs arrive in commit order and the advisory lock makes commit order equal id order.

The watermark, the page and the head come from one `REPEATABLE READ READ ONLY` snapshot (`listEventsPage` in `@ci/db`, Rust `list_events_after` plus the head). The **head** is the user's own newest id, raised to the watermark, never the table-wide maximum, so a stream reveals nothing about other users' activity and a resumed cursor is never below the watermark (no repeated resets). `oldestEventId` exists for Task 2 but does not decide `reset`: a user's oldest event says nothing about what was pruned.

**Bounds**, per stream:

| Limit | Value | Effect |
|---|---|---|
| Heartbeat | 15 000 ms | `: ping` comment |
| Maximum age | 30 min | stream closes; the browser reconnects with `Last-Event-ID`, which re-checks the session and picks up new links |
| Streams per user | 5 (per process) | the next one gets 429 `too_many_streams` |
| Queued bytes | 256 KiB | a client that stops reading is cut off |
| Buffered live events during a replay | 2 000 | stream closes; the client reconnects and replays |

On `request.signal` abort, cancel, any of the limits, or a failed replay, the stream unsubscribes, clears its timers and frees its slot.

**Poll mode** (`?after=<id>`): JSON `{ events: StreamEvent[]; lastId: number; reset: boolean }`, at most 500 events, `Cache-Control: no-store`. `lastId` is the last event's id, or `after` when nothing is new. On `reset` the events are left out and `lastId` is the head, like the SSE stream.

An account linked while a stream is open is picked up on the client's next reconnect (at the latest at the 30-minute maximum age). Pruning is a worker job (Rust `prune_events`, 7-day retention).

## Alternatives

- **Redis per-account channels, as the brief describes.** Rejected: the Rust publisher sends no Redis messages (ADR 0034), and Redis is being removed.
- **One LISTEN connection per SSE client.** Rejected: it costs a Postgres connection per tab, and the shared channel still carries every account's notifications to each of them.
- **`reset` when the cursor is below the user's oldest retained event (the brief's rule).** Rejected: it fires for any user whose first event came after the cursor, and differs from Rust's watermark rule.

## Consequences

- The stream cap and the hub are per process, which fits the single-process beta (ADR 0041). Several processes would need a shared cap.
- A session revoked, or an account unlinked, while a stream is open keeps streaming until the stream's maximum age (the account set is read once per stream).
- The hub's dispatch queue and the total number of streams across users are not capped; the per-user cap and invite-only access bound them for the beta.
- A truncated NOTIFY whose payload fetch keeps failing (after backoff retries) makes each affected stream buffer and replay from its last delivered id, so a live event is never skipped.
- Poll mode has no `hasMore` flag (the brief fixes the shape): a page of 500 events means poll again at once.
- With no linked accounts `listEventsPage` reports `reset = afterId < watermark`, whereas Rust's `list_events_after` returns false; harmless and consistent with the watermark rule.
- A replay failure closes the stream and EventSource reconnects after `retry: 3000`; there is no `error` event or extra throttle.
- Streams also refuse `Sec-Fetch-Site: cross-site` navigations, which carry the Lax cookie but no `Origin`.

## Rulings

Controller ruling (M4 T22 review): the Postgres LISTEN hub in place of the brief's Redis hub, and the watermark `reset` rule, are accepted per ADR 0034.
- Every payload sent to a browser is the stored or notified JSON. Event payloads must never carry data the account's users may not see.

## Client (M4 T23)

- **One stream per tab.** `RealtimeProvider` (signed-in shell only) mounts `useEventStream`, the app's single live-update mechanism. It applies each event to the TanStack cache with `applyEvent` (`lib/realtime/reducers.ts`); every type touches only its own keys, and `reset` is the one invalidation of everything. `LiveRefresh` (M5a T12, ADR 0056) stays as the bridge for server-component pages: it holds no connection, it only turns the stream's `games(accountId)` invalidation into a throttled `router.refresh()`.
- **No gap at start-up.** The layout reads `eventCursor(db, userId)` (`server/events/cursor.ts`) before any page data and passes it as `initialEventId`; the first stream opens with `?since=<id>`, so an event stored between the render and the stream opening is replayed. Browser reconnects send `Last-Event-ID`, which the server prefers. Without a cursor the server's `hello` supplies the head id.
- **Fallback.** Polling `?after=<lastEventId>` every 5000 ms when `EventSource` is missing, after 3 consecutive `error`s before any `open`, or when the browser closes the stream for good (a 429 or 503). A full page of 500 polls again at once. Every event with an id at or below `lastEventId` is dropped, live or polled.
- **Privacy.** The client never names a user or an account: the session alone decides which events arrive. `reconnectKey` (the joined account ids) only decides when to reopen.
- **Honest labels.** `ActivityIndicator` reports counts ("Importing 120 / 400 games", "3 jobs failed"), never a connection state; `useRealtime().mode` is the only place that knows whether updates are pushed or polled, and nothing may say "live" unless it is `"sse"`.
- **Server sync race (T22 re-review).** One sync runs at a time. A LISTEN rebuild during a sync sets a pending flag, and that sync replays again before flushing its buffer, so a buffered live event never jumps a gap (`test/events/resync-race.test.ts`).
