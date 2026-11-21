// RESERVED for the board and annotation lanes (Tasks 11 and 12), which replace this file with the
// live hero from DESIGN.md "Styleguide hero": the game played through its analysis reveal. Until
// then it holds the space at the real sizes, so the page never opens on a swatch grid.

const game = "Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858";

const moves: { n: string; white: string; black: string; glyph?: string; evaluation?: string }[] = [
  { n: "8.", white: "Nc3", black: "c6" },
  { n: "9.", white: "Bg5", black: "b5", glyph: "?", evaluation: "+5.7" },
  { n: "10.", white: "Nxb5", black: "cxb5", glyph: "?!", evaluation: "+8.1" },
];

const labels = ["4…Bxf3?!", "6. Bc4?!", "6…Nf6?", "9…b5?", "10…cxb5?!"];

function Placeholder({ children }: { children: string }) {
  return <span className="text-sm text-ink-faint">{children}</span>;
}

export function HeroSection() {
  return (
    <section
      id="hero"
      data-section="hero"
      aria-labelledby="hero-title"
      className="flex flex-col gap-5"
    >
      <h1 id="hero-title" className="max-w-4xl font-serif text-3xl font-semibold">
        {game}
      </h1>
      <div className="flex flex-col gap-5 md:flex-row md:items-start">
        <div
          className="grid aspect-square w-(--board-size) shrink-0 place-items-center rounded-board bg-well p-4 text-center"
          data-owner="board"
        >
          <Placeholder>Arrives with the board (Task 11/12)</Placeholder>
        </div>
        <div
          className="min-w-0 flex-1 rounded-sheet bg-sheet p-(--space-panel-pad)"
          data-owner="board"
        >
          <ol>
            {moves.map((move) => (
              <li
                key={move.n}
                className="flex h-(--space-scoresheet-row) items-center gap-3 border-b border-rule font-serif font-semibold tabular-nums lining-nums last:border-b-0"
              >
                <span className="w-8 font-normal text-ink-faint">{move.n}</span>
                <span className="w-16">{move.white}</span>
                <span>
                  {move.black}
                  {move.glyph && <span className="tracking-glyph text-pen-text">{move.glyph}</span>}
                  {move.evaluation && (
                    <span className="font-normal text-ink-faint"> {move.evaluation}</span>
                  )}
                </span>
              </li>
            ))}
          </ol>
          <p className="mt-4">
            <Placeholder>The full scoresheet arrives with the move list.</Placeholder>
          </p>
        </div>
      </div>
      <div data-owner="annotations">
        <ul
          className="flex h-5.5 flex-wrap items-end gap-x-4 overflow-hidden text-sm"
          aria-label="Marked moves"
        >
          {labels.map((label) => (
            <li key={label} className="font-serif font-semibold text-pen-text">
              {label}
            </li>
          ))}
        </ul>
        <div className="mt-1 grid h-22 place-items-center rounded-board bg-well md:h-30">
          <Placeholder>Arrives with the eval graph</Placeholder>
        </div>
      </div>
    </section>
  );
}
