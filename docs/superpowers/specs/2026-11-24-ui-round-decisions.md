# UI round: the owner's picks (v0.5 Board, sound and clarity)

Chosen by the owner on 2026-11-23 from rendered mockups. The mockups are the lab pages under `apps/web/src/app/lab/ui-round/` on branch `design/ui-round-mockups`, shot at 1440 and 390 px, day and night. These picks are binding for the v0.5 build.

## Information

- **Overview: option A now, moving towards B.**
  - **A:** opens on where you stand, the one thing to work on, and practise now. Everything else sits in a "More detail" list (progressive disclosure).
  - **B, later:** its one-sentence summary replaces step 1 once that sentence is checked against the facts (ADR 0104: the language layer never adds facts).
- **"Why this?":** opens in place on cards (overview, weakness pages). The side panel is used for the full ranking. Every variant shows the algorithm version line.

## The board and the trainer

- **The drill: option A's choreography with option B's session strip.**
  - **Wrong:** the piece springs back, dad's red pen `?` appears, and there's one retry.
  - **Right:** a tick and the sound.
  - **The strip:** B's 8-square session strip replaces a streak number.
- **The promotion picker: option A**, a column down the file on the promotion square, with 44 px targets on phones. B's number keys work from the keyboard, and Settings offers "Always a queen".
- **Board colourings and piece sets:** Slate stays the default and Pencil is the second colouring (Paper is dropped). Chessnut is the second piece set, as ADR 0039 plans.
- **The phone trainer: option C** (focus mode, no nav, full-width answers) with B's 384 px board, plus "Stop for today" and the count in view.

## Sound

- **Controls: option A.** A speaker in the header (mute and volume), plus one Settings page with three sections: Sound, Board and Moves (piece set, squares, promotion, premoves).
- **The sound set: "Felt" (option B)**, with A's capture knock for readable captures. The owner chose to go with it now and tune later.

## Found in the shipped app while mocking (fix in v0.5)

- The phone nav row has no Train link.
- The rating chart's labels collide at 390 px.

## Still open (the builder decides, then records the decision)

- The L-shaped knight arrow.
- Helper text at the `xs` size.
- An edge-to-edge phone board.
