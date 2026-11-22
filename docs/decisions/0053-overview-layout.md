# 0053. Overview layout

Date: 2026-11-22. Status: accepted (M5a Task 6, fix round 1).

## Context

Task 8 builds `/app`, the player's overview: accuracy over time, mistakes per game, results and score by time control, and the status of their games. Task 6 runs the lab procedure (LP, M4 plan "Shared procedures") on three layouts that differ on named axes:

- **a, a scoresheet column:** one reading column in which each KPI row carries its chart;
- **b, chart-led:** the accuracy trend full width on top, and from `lg` the KPI rows beside the judgement columns;
- **c, by time control first:** the time-control rows are the spine, and each opens to its accuracy and judgements.

All three run on the signed-in user's real data, and they 404 in production (`labEnabled`). They were built on the owner's analysed games, restored read-only into a throwaway compose project. The owner's games, their figures and screenshots of them are never committed.

## Decision

**Variant b.** An independent critic, working from screenshots only, ranked the variants **b > c > a** (`.superpowers/sdd/2026-11-21-05a-game-pages/critique-t6.md`). That matches the implementer's ranking. The record, with the reasons and the screenshot paths, is `packages/ui/design/decisions/overview.md`.

Task 8 takes from the losers, as the critic recommends:

- **from c:** the time-control split, as a segmented filter that drives the headline, timeline and rates, not an accordion; and c's results-bar, score, band and n row for "By time control";
- **from a:** the Recent games list as a ruled table, and "Score counts a draw as half a win".

The critic's top changes to b are hand-offs in `task-8-brief.md`:

1. a red-pen "what to work on" lead line;
2. honest KPI rulers and labelled sparklines;
3. no rolling average across months without games, with the chart aligned to the content column;
4. less repetition of n and of the headline numbers;
5. a smaller lead on phones;
6. readability fixes to the judgement columns and to dark-mode contrast.

Each variant keeps the honesty rules:

- n is shown beside every rate.
- Below 10 games the row reads "Need N more games" and shows no number.
- An interval that cannot be estimated is not drawn. The value is shown with "Every game gave the same value, so there is no interval", and nothing is ever widened.
- There are empty states: no linked account, no games, and none analysed.
- Phones show the latest 100 games in the trend, with "Show all".

**Client boundary (review-t6 Important 1).** A client component's props are serialised into the page. The client variants (b and c) therefore receive a narrow, JSON-plain view model built on the server (`toViewB` and `toViewC`). It holds only what they render: no raw rows, no opponents, no account ids, and dates as strings. A test holds those props to an allowlist of keys. Task 8 follows the same rule: the page stays a server component, and its client islands get view models.

## Consequences

- Task 8 implements b with the borrowed parts and the critic's hand-offs.
- `loadLabOverviewByClass` (variant c) runs one `getOverview` per time class. That is fine for a lab but not for a page, so Task 8 keeps `?tc=` and one query per request.
- The `@ci/ui/stats` text helpers (`bandText`) are client-only. A server component that needs them must be a client island, or the helpers must move to a shared module.
