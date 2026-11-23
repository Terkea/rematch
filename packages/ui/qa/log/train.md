# /app/train (the missed-moments trainer, ADR 0090)

## Round 1, 2026-11-23: self-critique only

No independent critic: the lane ran without subagents. Shots from `apps/web/e2e/train.spec.ts`
(`.shots/train/{find,confirm,reveal-wrong,summary}-{1440,390}-{light,dark}.png`), all with no
sideways scroll at 390 and 1440.

Findings and fixes:
1. The wrong answer's own move was not named in words, only as a pen arrow (colour alone). Added
   "Your move, 1. a4, is not one of the engine's good moves here." under the verdict.
2. An empty scope row added about 40px between the lede and the session when there is one scope.
   The row now renders only with more than one option.
3. "White to move. Move 1." repeated the board's name; the side panel now says "White to move."
4. An undrilled source showed "Not drilled yet." then "Before your first drill: …"; it now says
   "So far: …".
5. Shots were taken while the pen marks were still drawing; the spec waits for them.

Open: the e2e seed's positions are synthetic (an opening move judged a blunder), so the arrows in
the shots overlap on the a-file; real games do not look like this. Verdict: self-pass, needs an
independent critique before release.
