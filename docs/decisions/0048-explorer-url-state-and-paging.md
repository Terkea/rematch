# 0048. Games explorer: URL state, offset paging and UTC day filters

Date: 2026-11-22. Status: accepted.

## Context

The games explorer filters, sorts and pages the player's games. With no API routes (ADR 0046, R2), the page is a server component, so all list state has to live in the URL. URLs are shared, bookmarked and tampered with, and a stale or hand-edited URL must never error or leak.

## Decision

- **Param names.** `tc` (`bullet`, `blitz`, `rapid`, `classical`, `daily`), `result` (`win`, `draw`, `loss`), `colour` (`white`, `black`), `from` and `to` (`YYYY-MM-DD`), `account` (a linked account uuid), `sort` and `page`. `sort` is `played-desc` (the default), `played-asc`, `accuracy-desc`, `accuracy-asc`, `opponent-desc` or `blunders-desc`.
- **Parsing.** `parseGameQuery(params)` turns the search params into a `GameListQuery`. Every invalid value falls back to its default on its own: an unknown time class or sort (matched against an own-key allow-list, so `constructor` or `__proto__` never match), a repeated param, a date that is not a real calendar day between 2000 and 2100, a malformed uuid, or a page that is not a positive integer. A `from` later than `to` is swapped. A well-formed but foreign `account` is not rejected here; `scopedAccountIds` turns it into an empty list.
- **Serialising.** `serializeGameQuery(q)` writes only valid values (enum members, real days, uuids, integer pages of at least 2) and writes the keys in a fixed order (tc, result, colour, from, to, account, sort, page) and omits defaults (no sort for `played-desc`, no page for 1), so a parse and serialise round trip is stable and URLs stay short. Any filter change resets the page to 1.
- **R5, offset paging.** `PAGE_SIZE = 25`, a 1-based `page`, and a total count shown as "Showing 26–50 of 412 games". A page past the end clamps to the last page; an empty result is page 1 of 1. At a few hundred to a few thousand games per player, `count(*)` plus `limit/offset` is cheap and gives the total the copy needs; keyset paging would lose the total and the jump-to-page.
- **Sorting.** Ties always break on `played_at desc, id`, so on an unchanging set of games pages never overlap or skip. Offset paging does shift when games arrive between page loads (an import adds a game, and one row repeats on the next page); R5 accepts that for the total count and jump-to-page. Accuracy and blunders sort with nulls last in both directions: games whose analysis is not done have neither and go to the end. `opponent` sorts by the opponent's rating, nulls last.
- **R6, UTC days.** `from` and `to` are inclusive UTC days: `played_at >= from 00:00Z` and `played_at < (to + 1 day) 00:00Z`, so a game at 23:59:59Z on `to` is included and one at 00:00:00Z the next day is not. Dates are shown in UTC at day precision ("12 Nov 2026"), so the filter and the display agree for every viewer.

## Consequences

Tampered URLs render the defaults instead of an error. Players far from UTC may see a late-evening game under the next day; that is consistent across the filter and the display, and a per-user time zone can come later with its own ADR.
