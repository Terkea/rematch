import { Board, type BoardAnnotation } from "@ci/ui/board";

// The signed-out pages' diagrams, all from real positions (B15). The pen is reserved for a real
// mistake, so only sign-in and check-email carry marks; the other pages show the board at rest.
const GAME = "Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858";

const diagrams = {
  // After 9…b5?: the pen ring and `?` badge draw themselves once on load.
  mistake: {
    fen: "rn2kb1r/p3qppp/2p2n2/1p2p1B1/2B1P3/1QN5/PPP2PPP/R3K2R w KQkq - 0 10",
    label: `${GAME}, after 9…b5`,
    lastMove: { from: "b7", to: "b5" },
    annotations: [
      { kind: "circle", square: "b5", tone: "pen" },
      { kind: "mark", square: "b5", glyph: "?", tone: "pen" },
    ] as BoardAnnotation[],
    move: "9…b5",
    glyph: "?",
    evaluation: "+5.7",
    caption: GAME,
  },
  // The same game a move earlier, unmarked: a quiet board for a question that isn't about chess.
  still: {
    fen: "rn2kb1r/pp2qppp/2p2n2/4p1B1/2B1P3/1QN5/PPP2PPP/R3K2R b KQkq - 1 9",
    label: `${GAME}, after 9. Bg5`,
    lastMove: { from: "c1", to: "g5" },
    annotations: [] as BoardAnnotation[],
    move: "9. Bg5",
    glyph: null,
    evaluation: null,
    caption: GAME,
  },
  // Every game starts here: the page for someone who is welcome back later.
  start: {
    fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    label: "The starting position",
    lastMove: undefined,
    annotations: [] as BoardAnnotation[],
    move: null,
    glyph: null,
    evaluation: null,
    caption: "The starting position",
  },
} as const;

export type DiagramKind = keyof typeof diagrams;

export function OperaDiagram({ kind = "mistake" }: { kind?: DiagramKind }) {
  const d = diagrams[kind];
  return (
    <figure className="flex flex-col items-center gap-5">
      <Board
        fen={d.fen}
        label={d.label}
        {...(d.lastMove ? { lastMove: d.lastMove } : {})}
        annotations={d.annotations}
        animateAnnotations={d.annotations.length > 0}
      />
      <figcaption className="flex max-w-(--board-size) flex-col items-center gap-1 text-center">
        {d.move ? (
          <span className="font-serif text-xl font-semibold tabular-nums lining-nums text-ink">
            {d.move}
            {d.glyph ? <span className="tracking-glyph text-pen-text">{d.glyph}</span> : null}
            {d.evaluation ? (
              <span className="font-normal text-ink-faint"> {d.evaluation}</span>
            ) : null}
          </span>
        ) : null}
        <span className="text-sm text-ink-soft">{d.caption}</span>
      </figcaption>
    </figure>
  );
}
