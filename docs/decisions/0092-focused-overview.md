# 0092. A focused overview: trend, focus, practise, then the detail

Date: 2026-11-23. Status: accepted (overview lane, owner-approved: "a simpler overview: trend,
this week's focus, practise, goal progress, then the detail"). Recomposes the overview of ADR
0053 (variant b) using the trajectory card (ADR 0089), the weekly focus (ADR 0091), the trainer
(ADR 0090) and the filter bar (ADR 0083).

## Context

The end-user review of 2026-11-23 (item 4): an 800-level player meets seven areas, some 17
weaknesses, intervals and benchmarks on one page. They need "your one thing this week", a way to
practise it, and whether it is working, with the detail on demand.

## Decision

1. **Order.** The overview reads top to bottom:
   1. *Where you are*: the trajectory card, with the current rating, the peak-against-now
      headline and its n, and a link to Progress. Below the minimum it says how many more rated
      games it needs and shows no number.
   2. *This week's focus*: the focus card (goal and progress), or the suggestion with "Make this
      my focus".
   3. *Practise*: one primary call to action (below).
   4. *How you're doing*: the headline and the White | Black row, compact (the headline is one
      type step smaller than before).
   5. *Details*: "What to work on", accuracy over time, mistakes per game, by time control and
      recent games, as quieter sub-sections (`xl` headings, a `rule` line).
2. **Practise today.** `loadPractiseToday` reads the trainer's items without adding any and runs
   the same `buildSession` rules, so the count is the session the trainer serves: "Practise
   today: N positions due"; "All done for today, next due tomorrow" (or the day) when nothing is
   left; "Start training" when the trainer has made no item yet. The link is the trainer on the
   overview's scope, kept to the focus's source when that source has positions (otherwise every
   source). The focus card leaves out its own "Practise this" here, so the page has one primary
   action; Progress keeps it.
3. **Filters.** The time control scopes the whole page, so it sits under the title. The rest of
   the filter bar (format, colour, dates, compare) scopes Details and sits at its top, with the
   comparison right under it when active (critic 4). The summary's White | Black covers the whole
   time control, like the headline; with filters set, Details adds the filtered row ("By colour,
   with these filters") and a note says what the filters apply to.

## Consequences

- The first viewport answers "where am I, what is my one thing, and what do I do now".
- The overview's visual baselines change, and the filters e2e reads the time control and the
  filtered row in their new places.
