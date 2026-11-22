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
