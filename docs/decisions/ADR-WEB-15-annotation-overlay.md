# ADR-WEB-15. Annotation overlay: geometry, draw-on and placement rules

Date: 2026-11-21
Status: Accepted (Task 12, M4)

## Context

Task 12 builds the board's red-pen overlay (`packages/ui/src/annotations`) from DESIGN.md "Pen stroke geometry" and the Task 10 board decision. Implementing it on real positions raised cases the design law does not settle: knight arrows, a ring's overrun next to its halo, badges on squares that carry a coordinate or receive more than one arrow, and the opponent's marks in pencil.

## Decision

- **Layers of the API.** `arrowGeometry` returns the clean skeleton (one straight segment, or an L of two for a knight). `roughen(d, seed, amount)` bends each straight segment into a quadratic bowed 2–4% (seeded, endpoints kept, segment count kept). With a `seed`, `arrowGeometry` returns exactly `roughen(shaft, seed)` and fits the head to the bowed shaft's end tangent. `circlePath` is the ring's centreline (from about 1 o'clock, counter-clockwise as drawn, 380°). `strokeOutline` turns a centreline into the filled, tapered outline. Seeds: FNV-1a into mulberry32, keys `arrow:<from><to>` and `ring:<square>`.
- **Knight arrows** are an L with a rounded corner (radius 0.14 of a square). The two-square leg comes first, unless that corner square is the start or end of another arrow on the board; then the one-square leg comes first, so two arrows never share a leg (28. Nd2?? with Stockfish's 28. Kd2 in the reference game).
- **Draw-on.** Each stroke's outline is painted twice (`sheet` halo, then fill) through an SVG mask whose centreline is a Motion `motion.path` driven by `pathLength`. Strokes start `stagger.pen` apart in the order given, over `dur.draw` with `ease.pen`. Head strokes start when their shaft reaches 85% of its eased progress (266ms), take `dur.instant` each, 30ms apart. A good arrow's solid head is wiped on from its base. Badges land on `land` when the last stroke ends. A mask segment stays hidden until its stroke begins, because a zero-length dash with a round cap still paints a dot. `animate={false}` or reduced motion renders `pathLength` 1 at once; a badge remounts when that flips, so no spring is left running after hydration.
- **Ring overrun (changes the Task 10 note).** The ring is drawn as three mask pieces (0–180°, 180–360°, the 20° overrun) on one progress value, split by arc. All three pieces are one paint group (halos first, then fills), so the overrun inks over the start with no halo cut. Cutting it with its own halo, as the lab did, read as a broken, dashed ring at every size.
- **Badge corner** (fix round 1): the corner farthest from every point of every arrowhead that lands on the square (arrivals from the side, from above and on the diagonals), preferring top right, then top left, bottom right and bottom left. A corner that holds a coordinate (top left of the left column, bottom right of the bottom row) is never used.
- **Coordinates.** Strokes are cut by a mask where the coordinate glyphs sit (their boxes plus 2px), as DESIGN.md asks of every board mark. Badges are placed so they never need the cut.
- **Opponent's marks in pencil, with a badge.** The Task 13 ruling draws the opponent's mistakes in pencil. On the board a pencil `??` keeps its badge (filled `pencil`, `on-ink` glyph: 5.7:1 day, 7.6:1 night), because the glyph is what carries the severity. Pencil "plan or idea" marks still have none: the caller simply passes no `mark`.
- **Screen readers.** The SVG is `aria-hidden`; a visually hidden list reads "Red arrow from e4 to e6", "Green circle on f7", "Red ?? on e6" (pencil is "Grey").
- **Sizes.** The layer measures its square size (`ResizeObserver`, 56px until known) for the 1.1px minimum width, the 1px badge inset and the 11px minimum glyph. A resize re-plans the geometry without replaying the draw.

- **Short arrows** (fix round 1): a straight shaft is never shorter than its head plus 0.2 (`ARROW.minShaft`, 0.5). When the 0.22 start and the end clearance (0.16, or 0.46 at a ring) would leave less, both shrink in proportion. A one-square arrow into a ringed square keeps 0.5 of shaft, and its tip reaches slightly into the ring, which is painted over it.
- **Paint order** (fix round 1): ring halos, then each arrow (its halos, then its fills), then ring fills. Arrows still cut each other where they cross, but no arrowhead's halo notches a ring.
- **Pencil halo** (fix round 1): `HALO_SCALE` gives pencil half the halo (1px each side at 56px). The full halo around a 0.7× graphite line read as a tube at night.
- **Still rendering** (fix round 1): with `animate={false}` or reduced motion, strokes are plain paths with no reveal masks. Duplicate annotations are drawn and read once. The hidden list is labelled "Annotations on the board".
- **Notation** (fix round 1): `NotationText` takes `side` (`player` or `opponent`); the opponent's glyph is `pencil`, so red text, like a red mark, always means the player's mistake.
- **Lab** (fix round 1): `/lab/board/{a,b,c}` draw their marks with the real `AnnotationLayer`; their approximate pen code is gone, and the stroke comparison lives in `design/decisions/board.md`.

## Consequences

- The 12-mark animation budget (M9), retract and the link mark are the analysis reveal's job (later tasks); the layer only draws on.
- Drag tilt and the drag shadow layer (ADR-WEB-13 follow-up) stay open: they belong to the piece drag overlay, not the annotation layer.
