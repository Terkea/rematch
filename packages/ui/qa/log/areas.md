# Skill-area pages (`/app/areas/[area]`): critique log (M5b Task 8)

Screenshots are not committed. They were taken with `pnpm shots` (390 and 1440, light and dark,
reduced motion) from a local `next dev` with `LAB_ENABLED=1`, of a temporary, uncommitted lab route
that rendered the real `AreaView` with synthetic models: R3's synthetic report payload
(`backend/fixtures/ai/payload/report-payload.example.json`) and hand-made profile rows. No player
data. Three pages: Endgame (scored, a written note, three charted measures and one thin one, two
weaknesses with a timeline, three positions), Time (insufficient: "Need 9 more games for a
time-management score", thin measures, an early-read weakness) and Mindset (no score by design).

At 360 and 390 no page scrolls sideways (`scrollWidth` equal to the viewport); the area tabs and
the scope switch scroll inside their own tracks. The rendered text has no `NaN|undefined|Infinity|null`.

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the fresh critic passes (`critique-prompt.md`,
PNGs only, at least 2) are still owed: **the controller runs them and appends the verdicts here.**

`pnpm design:check`: 0 findings.

Issues found and fixed:

1. **The scope line read "From 214 games analysed rapid on lichess".** Now "From 214 analysed
   rapid games on lichess".
2. **Three example boards wrapped two and one at 1440.** The board's well adds 16px from 768px;
   `--board-size` is 20rem from 1024px, so three boards and their captions share one row of the
   1088px column. Phones keep the page's board token (one board per row).
3. **"Ask your coach about your time management" overflowed at 360 and 390** (a nowrap button).
   The button now wraps its label and grows in height.
4. **The thin-measure list stretched its label and shortfall 1,000px apart at 1440.** It is held
   to `max-w-3xl`.

Checked and kept:

- One layout for all seven areas, ruled sections down one column, score and note side by side
  from 1024px only. No card grid (B4), no stat banner (B9), no eyebrow (B5).
- A thin area states the shortfall and n in words ("11 analysed rapid games so far count towards
  it") and draws no ruler, so nothing reads as a score of 0.
- A thin measure shows its n and "Need N more games", never its value.
- The area score says "No interval yet": Rust's area score has no stored interval, so none is drawn
  and none is computed on the web.
- In the full-page captures the example boards' arrows are sometimes missing: the tall-viewport
  capture resizes the page and catches the arrows mid-redraw. A settled capture of one board shows
  all three arrows (pen, good, pencil).

## Fix round 1 (review-t8: code I1, I2, M1–M6; visual V1, V2, P1, P2, P6)

Shots: `scratchpad/m5b-t8/fix1/` (same synthetic models, plus each score's basis; Time now has a
report shortfall of 9 chances). No overflow at 360 or 390; no `NaN|undefined|Infinity|null`.

Fixed:

1. **V1. The area tabs and the scope switch on a phone.** Both tracks centre the current item on
   load (the tab you're on is in view on Time, Calculation and Mindset) and fade each edge that has
   more to scroll (a mask, so it works on any surface in both themes).
2. **V2/I1. "Likely range" in the score key with no range drawn.** The key now has no interval
   entry; "No interval yet" under the ruler states the gap.
3. **P1/I2. The caption's count.** No "last 50 games": Rust's area n is the smallest component
   sample, so the row names it in that component's unit ("46 rapid games", or "96 chances" for a
   rate component, or "31 in its thinnest measure" when no component row matches) and the caption
   names the measure ("The count is its thinnest measure, Endgame accuracy."). The report's
   shortfall is stated in its component's unit too ("Need 9 more chances for a time-management
   score"), and bare when none matches.
4. **P2.** A thin measure's status sits right after its label (a fixed label column), not 430px
   away.
5. **P6/M6.** "Ask your coach" and "Open this game at the move" reach 44px on touch.
6. **M1–M3.** A partial report without the area's note says the coach didn't write one (not
   "notes off"); an unscored area says "Your profile hasn't scored this area yet"; the report's
   shortfall is shown only when the report is at least as new as the live row.

Parked: P3 (timeline label column tint), P4 (hyphen in weakness-row ranges) and P7 (board
coordinates at 20rem) are upstream in `WeaknessTimeline`, `WeaknessRow` and `ExemplarBoard` (lane
U). P5 (a short Score column on Time and Mindset beside a taller note) is acceptable and now
carries the sample sentence. The thin-measure and weekly KPI counts still say "games" for every
KPI; the catalogue has no per-KPI count unit (rate KPIs count opportunities), so naming it needs a
Rust catalogue field.

Verdict: self-critique only; the controller's fresh critic passes are still owed.
