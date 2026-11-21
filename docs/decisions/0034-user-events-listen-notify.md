# 0034. User events over LISTEN/NOTIFY with a reset marker

Date: 2026-11-21
Status: Accepted. Implements 0030, keeps the ordering rule of 0029.

## Context

The web app shows import and analysis progress live, without blocking (SSE, with `Last-Event-ID` replay and a polling fallback). In the TS backend the worker inserted a `user_events` row under `pg_advisory_xact_lock(7262011)`, so ids become visible in id order, and then sent a Redis `PUBLISH` on `user-events:<accountId>` with a `createdAt` taken from `new Date()`, which is not the row's timestamp. Redis is removed in R1 (Task 27). Events older than 7 days are pruned, and a client whose `Last-Event-ID` predates the pruned range used to miss events silently.

## Decision

- **Delivery is Postgres `NOTIFY user_events`, sent inside the insert transaction.** `insert_user_event` (in `events::store`) takes the advisory lock, inserts `RETURNING id, created_at`, and calls `pg_notify('user_events', json)`, all in one transaction. It opens a savepoint when the caller is already in a transaction, so the event commits or rolls back with the state change it describes. Postgres delivers NOTIFYs only on commit, in commit order, and the lock makes commit order equal id order. A rolled-back insert notifies nobody. `publish` is the same call in its own transaction.
- **Payload format.** The NOTIFY body is camelCase JSON `{id, accountId, type, createdAt, payload}`. `createdAt` is the row's `created_at` returned by the INSERT. There is one canonical serialisation, `common::js_time::iso_micros`: UTC with exactly six fractional digits (`2026-11-21T10:00:05.123456Z`), the full precision of `timestamptz`. The NOTIFY, `LiveEvent` and `UserEventRow` all use it, and `impl From<UserEventRow> for LiveEvent` makes `LiveEvent` the single DTO. A live event and the same event replayed therefore serialise to byte-equal JSON. (`js_time::iso` is not used because it truncates to milliseconds. `Date.parse` reads both.)
- **The 8000-byte limit.** Postgres rejects NOTIFY payloads of 8000 bytes or more. When the serialised JSON exceeds 7900 bytes, `payload` is left out and `"truncated": true` is set. `EventListener::next` then reads the row by id, so callers always get the payload (or `None` if the row has already been pruned).
- **One channel, filtered by account.** All accounts share `user_events`. Each listener filters by account, so one LISTEN connection per web process serves every SSE client.
- **Watermark and reset rule.** `user_events_watermark.pruned_through_id` (one row) holds the highest id ever deleted by `prune_events`. It never moves backwards. `list_events_after` reads the watermark and the `id > after_id ORDER BY id LIMIT n` page in one `REPEATABLE READ READ ONLY` snapshot. It returns `reset = true` exactly when `after_id < pruned_through_id`, because the client may have missed a deleted event. Retention is `EVENT_RETENTION = 7 days`.
- **Lost connection.** If the LISTEN connection drops or fails, `EventListener::next` rebuilds it and runs LISTEN again. Only then does it return `Error::Reconnected`, so an event published after `Reconnected` is delivered live and also appears in the replay. If the rebuild fails, it returns `Error::Db` and the next call retries, then returns `Reconnected`. NOTIFYs sent during the gap are gone, so the caller replays from its cursor.
- **Cancel-safe `next`.** A truncated notification whose payload is being fetched stays in the listener until `next` returns it. A dropped `next` future (in `select!` or a timeout) loses nothing.
- **Insert last.** `insert_user_event` should be the last statement before the commit. Every writer queues on the one advisory lock, so taking it early holds up all other writers, and taking row locks after it can deadlock with a writer that holds those rows and waits for the lock.
- **Resources.** An `EventListener` holds one connection of its pool for its whole life, and briefly uses a second one to fetch a truncated payload.

## Web hand-off (M4 SSE route): the consumer protocol

The route holds one `LISTEN user_events` connection (`EventListener`) per process and filters notifications by the session's account ids. It writes `id:` lines, so the browser sends `Last-Event-ID` when it reconnects. For each client:

1. **Subscribe and buffer first.** Register with the listener and buffer its events before reading the table. A client without a `Last-Event-ID` reads its cursor, `SELECT coalesce(max(id), 0) FROM user_events`, *after* subscribing, so no event can fall between the cursor and the subscription.
2. **Replay.** Call `list_events_after(accounts, last_id, limit)` in a loop, advancing `last_id` to the last returned id, until a page has fewer than `limit` events. On `reset`, send a `reset` event: the client refetches its queries and the stream continues from the newest id.
3. **Drain, then go live.** Send the buffered events, dropping each `id <= last_id`. Then send live events, still dropping `id <= last_id` and advancing `last_id`.

This dedupe is exact because live delivery is in id order. The advisory lock makes commit order equal id order, and Postgres delivers NOTIFYs in commit order. Once an id above `last_id` has arrived, no lower id can follow.

How to handle errors from `next`:
- `Reconnected`: the LISTEN is already back. Repeat step 2 from `last_id` for every client, then continue.
- `Db`: a failed reconnect or payload fetch. Keep calling with a backoff. Nothing is lost.
- `Json`: one malformed message was skipped. Keep calling.

## Consequences

- No broker is needed for live updates, and live and replayed events are the same rows with the same timestamps.
- Every event insert is serialised by one advisory lock. The rate is small (progress ticks and per-game results), and the lock is held only until the surrounding transaction commits, so such transactions should stay short.
- Pruned events cannot be replayed, but clients are told (`reset`) instead of missing them.
