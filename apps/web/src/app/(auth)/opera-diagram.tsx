import { Board } from "@ci/ui/board";

// Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858, after 9…b5? (the game the
// design references use, with the evaluation from the styleguide hero). The pen ring and the `?`
// badge draw themselves once on load: the product's signature, on a real mistake (B15).
const FEN = "rn2kb1r/p3qppp/2p2n2/1p2p1B1/2B1P3/1QN5/PPP2PPP/R3K2R w KQkq - 0 10";

export function OperaDiagram() {
  return (
    <figure className="flex flex-col items-center gap-5">
      <Board
        fen={FEN}
        label="Morphy v Duke Karl of Brunswick and Count Isouard, after 9…b5"
        lastMove={{ from: "b7", to: "b5" }}
        annotations={[
          { kind: "circle", square: "b5", tone: "pen" },
          { kind: "mark", square: "b5", glyph: "?", tone: "pen" },
        ]}
        animateAnnotations
      />
      <figcaption className="flex max-w-(--board-size) flex-col items-center gap-1 text-center">
        <span className="font-serif text-xl font-semibold tabular-nums lining-nums text-ink">
          9…b5<span className="tracking-glyph text-pen-text">?</span>
          <span className="font-normal text-ink-faint"> +5.7</span>
        </span>
        <span className="text-sm text-ink-soft">
          Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858
        </span>
      </figcaption>
    </figure>
  );
}
