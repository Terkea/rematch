# 0034. User events over LISTEN/NOTIFY with a reset marker

Date: 2026-11-21
Status: Accepted. Implements 0030, keeps the ordering rule of 0029.

## Context

The web app shows import and analysis progress live, without blocking (SSE, with `Last-Event-ID` replay and a polling fallback). In the TS backend the worker inserted a `user_events` row under `pg_advisory_xact_lock(7262011)`, so ids become visible in id order, and then sent a Redis `PUBLISH` on `user-events:<accountId>` with a `createdAt` taken from `new Date()`, which is not the row's timestamp. Redis is removed in R1 (Task 27). Events older than 7 days are pruned, and a client whose `Last-Event-ID` predates the pruned range used to miss events silently.

## Decision

- **Delivery is Postgres `NOTIFY user_events`, sent inside the insert transaction.** `insert_user_event` (in `events::store`) takes the advisory lock, inserts `RETURNING id, created_at`, and calls `pg_notify('user_events', json)`, all in one transaction. It opens a savepoint when the caller is already in a transaction, so the event commits or rolls back with the state change it describes. Postgres delivers NOTIFYs only on commit, in commit order, and the lock makes commit order equal id order. A rolled-back insert notifies nobody. `publish` is the same call in its own transaction.
- **Payload format.** The NOTIFY body is camelCase JSON `{id, accountId, type, createdAt, payload}`. `createdAt` is the row's `created_at` returned by the INSERT, written in RFC 3339 with every fractional digit, so it equals the stored value to the microsecond. (It does not use `js_time::iso`, which truncates to milliseconds. `Date.parse` still reads it.)
- **The 8000-byte limit.** Postgres rejects NOTIFY payloads of 8000 bytes or more. When the serialised JSON exceeds 7900 bytes, `payload` is left out and `"truncated": true` is set. `EventListener::next` then reads the row by id, so callers always get the payload (or `None` if the row has already been pruned).
- **One channel, filtered by account.** All accounts share `user_events`. Each listener filters by account, so one LISTEN connection per web process serves every SSE client.
- **Watermark and reset rule.** `user_events_watermark.pruned_through_id` (one row) holds the highest id ever deleted by `prune_events`. It never moves backwards. `list_events_after` reads the watermark and the `id > after_id ORDER BY id LIMIT n` page in one `REPEATABLE READ READ ONLY` snapshot. It returns `reset = true` exactly when `after_id < pruned_through_id`, because the client may have missed a deleted event. Retention is `EVENT_RETENTION = 7 days`.
- **Lost connection.** If the LISTEN connection drops, `EventListener::next` returns `Error::Reconnected` once, after re-listening. NOTIFYs sent during the gap are gone, so the caller replays with `list_events_after` from its cursor. Replay is exactly-once and in order, because every writer goes through `insert_user_event`.

## Web hand-off (M4 SSE route)

The route holds one `LISTEN user_events` connection per process. It filters notifications by the session's account ids and writes `id:` lines so the browser sends `Last-Event-ID` on reconnect. On connect, and after `Reconnected`, it replays with `list_events_after(accounts, last_id)`. A client without a `Last-Event-ID` starts at the current maximum id, because its state was just fetched. On `reset` it sends a `reset` event, and the client refetches its queries and continues from the newest id. Live events with `id <= last_id` are dropped, which removes the overlap between replay and live delivery.

## Consequences

- No broker is needed for live updates, and live and replayed events are the same rows with the same timestamps.
- Every event insert is serialised by one advisory lock. The rate is small (progress ticks and per-game results), and the lock is held only until the surrounding transaction commits, so such transactions should stay short.
- Pruned events cannot be replayed, but clients are told (`reset`) instead of missing them.
