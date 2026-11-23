# The public site (`site/`, landing page and docs): critique log (ADR 0096)

Screenshots are not committed. They were taken headless from `astro preview` of a production
build (`pnpm --filter @ci/site build`), full page at 390 and 1440, day and night, of the landing
page (`/rematch/`), Getting started and one generated decision page. The feature screenshots on
the page are the app's `/styleguide` fixture sections, captured at 2x; nothing comes from `/app` or
a signed-in session. At 360 and 390 no page scrolls sideways (`scrollWidth` equal to the viewport),
and the capture script fails on any visible image with `naturalWidth` 0.

`pnpm design:check` (now scanning `site/src`, `.astro` included): 0 findings.

## Round 1 (fresh critic, PNGs, DESIGN.md and REFERENCES.md only): `revise`

1. **Blocker: blank feature images in the captures.** Lazy images below the fold were never
   scrolled into view by the full-page capture. The capture now scrolls the page, decodes every
   visible image and fails on a blank one.
2. **Major: the day poster glares on the night page.** Partly accepted: the film is a day-theme
   recording with no night render. The poster is now the film's hook frame (the pen-ringed board
   and "Same mistake. Different game."), which is the film's own first beat.
3. **Major (B5): the old poster's tracked caps "CHESS IMPROVER" and the repeated tagline.** Fixed
   by the new poster frame, which has neither.
4. **Major: screenshots scaled to 5–7px text.** Recaptured at 2x and cropped to one readable
   region each (the board and scoresheet, two weakness rows, one focus card, the trajectory card),
   shown at natural size or smaller, never upscaled. Wide shots (the eval graphs, weakness rows,
   trajectory) get the full width under their copy, and on a phone they keep a readable size and
   scroll inside their own frame.
5. **Minor (B4 drift): thick well bezels on every image.** Each shot is now its own `sheet` with
   one 1px `rule` edge and `--radius-sheet`.
6. **Minor: "Lichess blitz" in the chess.com feature shot.** The crop now shows the board and
   scoresheet only.
7. **Minor (B5): the docs pagination arrow and sans title.** The arrow is hidden and the title is
   Literata 600.
8. **Minor: the filled phone menu button and clipped code.** The button is a ghost control with a
   `rule-strong` edge; code blocks wrap.
9. **Minor: sidebar dividers stop partway down.** A full-page capture artefact of a sticky element;
   not reproducible in a viewport-height view.
10. **Minor: the demo-data caveat twice; generic contribute heading.** The caveat is said once (the
    tour's lead); the film's caption now says what the film shows. The heading is the owner's
    wording and stays.

## Round 2 (a second fresh critic): `revise`, "fix the two majors and this passes"

1. **Major: on phones the coach, focus and positions shots shrank to 5–7px text.** Every shot but
   the review board now keeps its natural size on a phone (the three wide ones 960px) and scrolls
   sideways inside its own focusable frame.
2. **Major: the weakness track and its "20.0%" were clipped at the frame's edge.** Recaptured 25px
   wider, so the track's end and label sit inside.
3. **Minor (B15): the styleguide's synthetic, sawtooth rating history.** Accepted for now: it is the
   styleguide's labelled demo fixture, and the page says so once. A realistic fixture belongs to the
   trajectory component's own lane.
4. **Minor: "1 , 111" in the chart and "1 ." list markers.** The chart is the app's own component
   (out of this lane); docs list markers now reset numeric variants and spacing.
5. **Minor: wrapped code on phones.** Code keeps its lines and scrolls sideways (round 1 had asked
   for wrapping; a wrapped command is the worse failure).
6. **Minor: the same mould for every feature.** Two features (weaknesses, trajectory) already run
   full width; the rest keep the alternating pair. Left as is.
7. **Minor (B4): five identical diagram boxes.** Nodes now differ by role: chess.com, the outside
   source, is drawn in pencil; Postgres, the store, is a `well`; the AI branch stays dashed.
8. **Minor: the play pill covered the ringed d2 on phones.** It sits at the bottom right, over the
   empty paper.
9. **Minor: no chess touch in the docs.** Left for a later pass.

## Round 3 (a third fresh critic): `revise` on one major, the rest minor

1. **Major (B5): "monospace" numbers in the rating-trajectory chart.** Not a monospace face: the
   app's `RatingTrajectory` (`packages/ui/src/charts/rating-trajectory.tsx`) sets its ticks and its
   peak and now labels in Schibsted with `tabular-nums`, which gives the comma a figure's width
   ("1 , 200"). It is the app's own component, shown as captured; changing it moves the Progress
   baselines, so it is out of this lane and left as a follow-up for the chart (tabular figures on
   the digits only, or proportional figures for these labels). Ruling: accepted for the site.
2. **Minor (B3): ruled roadmap columns.** The rules between items are gone; one `rule-strong` line
   stays under each column heading.
3. **Minor (B4): diagram nodes styled by role read as states.** Back to one treatment; only the
   optional AI branch is dashed, and its text says it is a branch of the worker.
4. **Minor: the poster's title card beside the H1; the play pill over the board on phones.** The
   poster is the film's own first beat and stays (every hook frame carries the title card). Below
   480px the play control sits under the poster.
5. **Minor: copy centred against tall shots; the weakness shot centred on its own axis.** Copy is
   top-aligned, and shots align to their column's start.
6. **Minor: inline code chips read as stray spaces.** Padding is 0.15em with no margin.
7. **Minor: aligned shell comments in a proportional face.** Each comment is on its own line.
8. **Minor: stock Starlight chrome.** Left: the docs keep Starlight's header with the app's type,
   colours and controls.
9. **Minor: the AI node in the phone flow.** Below 1024px it comes after the chain, with no
   connector.
10. **Minor: "Security" alone on the footer's second row.** Left: links wrap naturally.

Verdict after round 3: no blocker; the one major is the app chart's figure spacing, ruled out of
this lane above. Every other finding is fixed or ruled.

## How it works: the landing section, the docs page and the shared drawing (docs/how-it-works)

Shots: headless Chromium against `astro preview` of a production build, the landing's
`#how-it-works` section and `/rematch/docs/how-it-works/` (top, one disclosure open, the
thresholds table) at 390 and 1440, day and night (16 PNGs, not committed). `scrollWidth` equals
the viewport in every shot. `pnpm design:check`: 0 findings.

### Round 1 (fresh critic, PNGs, DESIGN.md and REFERENCES.md only): `revise`

1. Blocker: the wide SVG scaled to 390 put labels near 8–10px. Fixed: under 640px a second,
   one-column drawing puts each stage's inputs inside it as chips; every label stays on the type
   scale.
2. Major: stray spaces around commas and full stops in table cells. Cause: `tabular-nums` gives
   Schibsted's punctuation figure widths (the same cause as round 3's chart finding). Fixed:
   docs tables use lining figures only.
3. Major: the three-column thresholds table clipped at 390. Fixed: two columns.
4. Major: formula blocks clipped. Fixed: shorter lines, and `pre-wrap` inside disclosures.
5. Major: the docs page opened with the "generated from README" note. Fixed: a lede first; the
   note moved to the end in `ink-faint`.
6. Major (B4): the disclosures read as a FAQ accordion. Fixed: summaries one step down
   (Schibsted 600 `base`), each a `sheet` card with a rule under an open summary.
7. Major: contributor vocabulary on the landing. Fixed: the intro starts from the player; the
   drawing's subtitles are plain words.
8. Minor: the list centred against the drawing, the link detached. Fixed: top-aligned, the link
   ends the list.
9. Minor: faint dotted strokes at night. Fixed: `pencil` at 1.5px, 4 3 dashes, and "optional" in
   the optional boxes' subtitles.
10. Minor: a stock flowchart; the rotated Postgres label. The engine stage now carries the README's
    worked example, 28. Nd2?? (a real Lichess blitz game, the styleguide's analysed fixture).
    Ruling: the rotated rail stays on the wide drawing, where it reads; the phone drawing drops it
    for a caption line.

### Round 2: `revise`, one major

1. Major: 28. Nd2?? in bold sans and all pen red. Fixed: Literata 600, `ink` move with only `??` in
   `pen-text`, the "73% to 40%: a blunder" line in `ink-soft` on both drawings, a leader on the wide
   one.
2–4. Minor: the lede's step count, an awkward intro line, two names (habit, weakness). Fixed.
5. Minor (optional): separate cards. Ruled: kept; each stage is its own disclosure.
6. Minor: inline-code chip padding after "(". Ruled: site-wide chip style, ruled in round 3 above.

### Round 3: `pass`

Two minors. The phone Stockfish box gets 5px more above the notation line. The example comes from
a Lichess game shown beside a chess.com pipeline: the README's worked example says so ("a real
Lichess blitz game"), and the drawing names no platform for it, so it stays.
