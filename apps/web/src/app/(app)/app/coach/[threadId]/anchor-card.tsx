import type { Db } from "@ci/db";
import { Board, formatNotation } from "@ci/ui/board";
import { dayText } from "@ci/ui/charts";
import Link from "next/link";
import type { ChatAnchor } from "../../../../../server/coach/anchor";
import type { SectionKey } from "../../../../../server/coaching/types";
import { getGameReview, type ReviewGame } from "../../../../../server/games/review";
import { seriesLabel, WEAKNESSES, weaknessDef } from "../../../../../server/profile/catalogue";
import { reviewGameIds } from "../../../../../server/profile/game-ids";

// What a conversation was opened from (Task 9): the position as a small board for a move or a
// position, the weakness by name, the report section, or the game in one line. Everything is
// read for the signed-in user (`getGameReview` is scoped to their accounts), so an anchor on a
// game they cannot see resolves to nothing and the page drops it. An anchor names its game by
// the PLATFORM id, as the worker resolves it (`get_position`); the review takes `games.id`.

type Square = `${"a" | "b" | "c" | "d" | "e" | "f" | "g" | "h"}${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8}`;

export type AnchorView =
  | {
      kind: "move" | "position";
      fen: string;
      orientation: "white" | "black";
      lastMove: { from: Square; to: Square } | null;
      /** "Position after 28. Nd2": the board's name and its caption. */
      label: string;
      game: string;
      href: string;
    }
  | { kind: "game"; game: string; href: string }
  | { kind: "weakness"; label: string }
  | { kind: "report_section"; label: string }
  | { kind: "opening_line"; label: string };

const SECTION: Record<SectionKey, string> = {
  overview: "Overview",
  weaknesses: "Weaknesses",
  openings: "Openings",
  climb_plan: "Climb plan",
  areas: "Areas",
};

const HEADING: Record<AnchorView["kind"], string> = {
  move: "About a move",
  position: "About a position",
  game: "About a game",
  weakness: "About a weakness",
  report_section: "From your report",
  opening_line: "About an opening",
};

const SQUARE = /^[a-h][1-8]$/;

/** "7 Nov 2026, loss v Blacky (1510)": the game from the player's side. */
export function gameLine(g: ReviewGame): string {
  const opp = g.playerColor === "white" ? g.black : g.white;
  const rating = opp.rating === null ? "" : ` (${opp.rating})`;
  return `${dayText(g.playedAt)}, ${g.playerResult} v ${opp.name}${rating}`;
}

function weaknessName(id: string): string {
  const def = weaknessDef(id) ?? WEAKNESSES.find((w) => w.contractId === id);
  if (def) return def.label;
  const label = seriesLabel(id);
  return label === id ? "One of your weaknesses" : label;
}

/** The review game of a platform id on the conversation's account, or null. */
async function reviewGame(
  db: Db,
  userId: string,
  accountId: string,
  platformId: string,
): Promise<ReviewGame | null> {
  const id = (await reviewGameIds(db, userId, { accountId }, [platformId])).get(platformId);
  return id ? getGameReview(db, userId, id) : null;
}

/**
 * The anchor as the card shows it, or null when it cannot be shown for this user on
 * `accountId` (the conversation's account, which the worker binds the anchor to).
 */
export async function loadAnchorView(
  db: Db,
  userId: string,
  accountId: string,
  anchor: ChatAnchor,
): Promise<AnchorView | null> {
  switch (anchor.kind) {
    case "move":
    case "position": {
      const g = await reviewGame(db, userId, accountId, anchor.gameId);
      const p = g?.plies[anchor.ply - 1];
      if (!g || !p) return null;
      const from = p.uci.slice(0, 2);
      const to = p.uci.slice(2, 4);
      const move = formatNotation({ moveNumber: Math.ceil(p.ply / 2), color: p.color, san: p.san });
      return {
        kind: anchor.kind,
        fen: p.fenAfter,
        orientation: g.playerColor,
        lastMove:
          SQUARE.test(from) && SQUARE.test(to) ? { from: from as Square, to: to as Square } : null,
        label: `Position after ${move}`,
        game: gameLine(g),
        href: `/app/games/${encodeURIComponent(g.id)}?ply=${p.ply}`,
      };
    }
    case "game": {
      const g = await reviewGame(db, userId, accountId, anchor.gameId);
      if (!g) return null;
      return { kind: "game", game: gameLine(g), href: `/app/games/${encodeURIComponent(g.id)}` };
    }
    case "weakness":
      return { kind: "weakness", label: weaknessName(anchor.weaknessId) };
    case "report_section":
      return { kind: "report_section", label: SECTION[anchor.section] };
    case "opening_line": {
      const label = seriesLabel(anchor.lineKey);
      return {
        kind: "opening_line",
        label: label === anchor.lineKey ? "One of your opening lines" : label,
      };
    }
  }
}

const linkClass =
  "inline-flex min-h-11 items-center font-sans text-sm text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink";

export function AnchorCard({ view }: { view: AnchorView }) {
  const heading = HEADING[view.kind];
  return (
    <section
      aria-label={heading}
      data-surface="well"
      className="flex flex-col gap-4 rounded-sheet bg-well p-(--space-panel-pad) sm:flex-row sm:items-start"
    >
      {view.kind === "move" || view.kind === "position" ? (
        <div
          // ban-lint-allow TOK2: a reference board fills its small column in whole-pixel squares
          className="@container w-48 shrink-0 [--board-size:round(down,100cqi,8px)] md:w-52 md:[--board-size:round(down,100cqi_-_16px,8px)]"
        >
          <Board
            fen={view.fen}
            label={view.label}
            orientation={view.orientation}
            {...(view.lastMove ? { lastMove: view.lastMove } : {})}
          />
        </div>
      ) : null}
      <div className="flex min-w-0 flex-col gap-1">
        <h2 className="m-0 font-sans text-sm font-medium text-ink-soft">{heading}</h2>
        {view.kind === "move" || view.kind === "position" ? (
          <>
            <p className="m-0 font-serif text-lg text-ink">{view.label}</p>
            <p className="m-0 font-sans text-sm text-ink-soft">{view.game}</p>
            <Link href={view.href} className={linkClass}>
              Open the game at this move
            </Link>
          </>
        ) : view.kind === "game" ? (
          <>
            <p className="m-0 font-serif text-lg text-ink">{view.game}</p>
            <Link href={view.href} className={linkClass}>
              Open the game
            </Link>
          </>
        ) : (
          <p className="m-0 font-serif text-lg text-ink">{view.label}</p>
        )}
      </div>
    </section>
  );
}
