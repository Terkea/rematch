import Link from "next/link";
import type { ReactNode } from "react";
import { brand } from "../../brand";
import {
  CBURNETT_BSD,
  CHESSJS_BSD2,
  LITERATA_OFL,
  PHOSPHOR_MIT,
  REACT_CHESSBOARD_MIT,
  SCHIBSTED_OFL,
} from "./texts";

export const metadata = { title: "Licences" };

const link =
  "underline decoration-rule-strong underline-offset-3 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink rounded-field";

function Credit({
  id,
  title,
  children,
  text,
}: {
  id: string;
  title: string;
  children: ReactNode;
  text?: string;
}) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-3 border-t border-rule pt-8">
      <h2 id={id} className="font-serif text-lg font-semibold text-ink">
        {title}
      </h2>
      <div className="flex flex-col gap-2 text-ink-soft">{children}</div>
      {text ? (
        <details className="rounded-field bg-well">
          <summary className="cursor-pointer rounded-field px-4 py-3 text-sm font-medium text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink">
            The licence text
          </summary>
          <p className="px-4 pb-4 text-xs leading-relaxed whitespace-pre-wrap text-ink-soft [overflow-wrap:anywhere]">
            {text}
          </p>
        </details>
      ) : null}
    </section>
  );
}

/**
 * /licenses: the third-party work the app ships, with its attribution and licence text (ADR 0039
 * and its addendum, ADR-WEB-7, ADR-WEB-13). Public, like the sign-in pages. The texts are
 * verbatim; `details` keeps the page readable while every word stays in the document.
 */
export default function LicensesPage() {
  return (
    <div className="flex min-h-dvh flex-col bg-paper">
      <div className="px-(--space-gutter) pt-6">
        <Link
          href="/"
          className="rounded-field font-serif text-xl font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          {brand.wordmark}
        </Link>
      </div>
      <main className="mx-auto flex w-full max-w-2xl flex-col gap-8 px-(--space-gutter) pt-10 pb-16">
        <header className="flex flex-col gap-2">
          <h1 className="font-serif text-2xl font-semibold text-ink">Licences</h1>
          <p className="text-ink-soft">
            The pieces, board, rules, typefaces, icons and opening names in {brand.wordmark} come
            from other people's work. Here is who made them, and the terms we use them under.
          </p>
        </header>

        <Credit id="pieces" title="Chess pieces: Cburnett" text={CBURNETT_BSD}>
          <p>
            Drawn by Colin M.L. Burnett, from{" "}
            <a className={link} href="https://commons.wikimedia.org/wiki/Category:SVG_chess_pieces">
              Wikimedia Commons
            </a>
            . He offers them under several licences; we use the BSD 3-Clause licence. We recolour
            them to our board's ink.
          </p>
          <p>
            The board library react-chessboard (MIT, Ryan Gregory) bundles its own copy of the same
            drawings. It is never shown, and this notice covers it too.
          </p>
        </Credit>

        <Credit id="literata" title="Typeface: Literata" text={LITERATA_OFL}>
          <p>By the Literata Project Authors, under the SIL Open Font License 1.1.</p>
        </Credit>

        <Credit id="schibsted" title="Typeface: Schibsted Grotesk" text={SCHIBSTED_OFL}>
          <p>By the Schibsted Grotesk Project Authors, under the SIL Open Font License 1.1.</p>
        </Credit>

        <Credit id="icons" title="Icons: Phosphor" text={PHOSPHOR_MIT}>
          <p>Phosphor Icons, under the MIT License.</p>
        </Credit>

        <Credit id="chessjs" title="Chess rules: chess.js" text={CHESSJS_BSD2}>
          <p>By Jeff Hlywa, under the BSD 2-Clause licence.</p>
        </Credit>

        <Credit id="board" title="Board: react-chessboard" text={REACT_CHESSBOARD_MIT}>
          <p>By Ryan Gregory, under the MIT License.</p>
        </Credit>

        <Credit id="openings" title="Opening names">
          <p>
            From{" "}
            <a className={link} href="https://github.com/lichess-org/chess-openings">
              lichess-org/chess-openings
            </a>
            , dedicated to the public domain under CC0 1.0 Universal. Thank you to its contributors.
          </p>
        </Credit>

        <Credit id="engines" title="Analysis engines">
          <p>
            Games are analysed on our servers by{" "}
            <a className={link} href="https://github.com/official-stockfish/Stockfish">
              Stockfish
            </a>{" "}
            (GPL-3.0) and{" "}
            <a className={link} href="https://github.com/CSSLab/maia2">
              Maia-2
            </a>{" "}
            (MIT). Neither is sent to your browser.
          </p>
        </Credit>
      </main>
    </div>
  );
}
