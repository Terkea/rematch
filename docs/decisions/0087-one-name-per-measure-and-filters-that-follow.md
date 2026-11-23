# 0087. One plain name per measure, and filters that follow the player

Date: 2026-11-23. Status: accepted (fix lane L, the gap audit A1-A15 and Q7-Q8). Builds on ADR
0068 (the catalogue mirror), ADR 0079 (explained stats), ADRs 0080 and 0081 (the peer
benchmark), ADR 0082 (the colour split), ADR 0083 (the filter URL contract) and ADR 0085 (the
coach's colour split and glossary). Number 0086 is taken by another lane.

## Context

The owner's complaint: the same weakness had two names. Rows, the detail header and the coach
input used the catalogue label ("Conversion failure", "Opening traps"); popovers and the
report's colour section used the glossary name ("Not winning won games", "Early blunders"). All
19 weaknesses and 14 KPIs differed. Separately, the colour and the dates were lost on most page
changes and on "Ask your coach", Strengths had no filters at all and read colour rows into its
Both view, and the report still described area scores in the old Lichess band's words.

## Decision

1. **The glossary name wins.** It is the plainer one. Rust's `weakness_label`, `motif_label`
   and the KPI rows' labels now carry the glossary names, so the web catalogue (its snapshot),
   the coaching input, the report titles and the coach all say the same words. Motif series in
   the coaching input use the pages' names too ("Missed forks", "Allowed free pieces"), no
   longer "Missed tactic: fork". `catalogue.test.ts` asserts every catalogue label equals its
   glossary name, so the two cannot drift again; the glossary drift test (detector constants)
   is unchanged. The coach's glossary prompt does not change: it already used these names.
2. **Strengths reads the colour and dates it shows.** Peer, band and KPI rows are read for the
   scope's side only (Both reads the both rows). Weakness changes come from the both estimates,
   since the timeline is every game's. A colour or a date range narrows the per-game reads (the
   comparison with earlier games, the openings, the hard spots); the stored last-50 rows and the
   timeline have no dates, and the page says so. Both mode adds a "By colour" block: each
   colour's own comparison with other players. Every claim row has a "What is this?".
3. **Filters follow the player.** The header, the command menu, the account menu and the
   `/app/areas` redirect carry `account`, `tc`, `color` and `from`/`to` (`withFilters`): the
   games page in its own words (`colour`, no account), the coach pages only the scope (a new
   chat also the colour), other pages nothing. Area tabs keep the dates.
4. **Ask your coach keeps the colour.** `coachHref` adds a one-colour scope's `color`. The new
   chat page says the question is about that colour, and the first question is sent as "About
   my games with White: ..." in the player's visible words, so the coach answers from
   `byColor` (already in the chat context, ADR 0085). No anchor or schema change was needed;
   the colour is part of what the player asked. Dates are not carried: the coach context is not
   dated.
5. **Dates say what they change.** Weaknesses and Areas state that the dates trim the weekly
   lines only, and what stays on its window (6 months, last 50 games).
6. **A colour's benchmark counts its own players.** The label in colour mode uses the widest
   of that side's peer rows for its player and game counts, not the both header's.
7. **Area scores are whole numbers and name their benchmark (Q7, Q8).** `AreaScore` rounds
   every number it shows or reads aloud (intervals outward). With a peer benchmark, the
   report's "Where you stand" and the area page name the players faced and say a score is the
   distance from their rate given how well it is known, not a rank (ADR 0081).

## Consequences

- Stored report titles from before this change keep the old labels until the next report.
- A question sent from a colour's page carries the colour words in the stored message.
- Visual baselines whose pages show a weakness or KPI name change once.
