// Fixture data for the ui-round mockups. The chess is real: positions come from the committed
// fixture games (@ci/ui/fixtures, public Lichess games). The overview numbers belong to a
// fictional player, shaped like the demo seed's (ADR 0097), and every page that shows them says
// so in its lab chrome. Nothing here reads /app or any user's data.

/** endgame-grind: DrNykterstein v ChessArtHouse, Lichess bullet, https://lichess.org/zR8HRccT. */
export const DRILL = {
  game: "DrNykterstein v ChessArtHouse, Lichess bullet",
  source: "https://lichess.org/zR8HRccT",
  /** After 63… Ka4: White to move, mate in one (Stockfish, from the fixture's eval). */
  fen: "8/8/6K1/3N4/kB6/P2Q4/8/8 w - - 7 64",
  lastMove: { from: "b5", to: "a4" } as const,
  moveNumber: 64,
  /** What was played in the game: mate in three instead of one. */
  wrong: {
    san: "Qc3",
    from: "d3",
    to: "c3",
    piece: "wQ",
    fenAfter: "8/8/6K1/3N4/kB6/P1Q5/8/8 b - - 8 64",
  },
  right: {
    san: "Nb6#",
    from: "d5",
    to: "b6",
    piece: "wN",
    fenAfter: "8/8/1N4K1/8/kB6/P2Q4/8/8 b - - 8 64",
  },
} as const;

/** endgame-grind ply 119: White's h-pawn promotes (the game played h8=Q). */
export const PROMOTION = {
  fen: "8/7P/6K1/8/kB3N2/P7/8/8 w - - 1 60",
  lastMove: { from: "b3", to: "a4" } as const,
  from: "h7",
  to: "h8",
  label: "Position after 59… Ka4. White's pawn on h7 is about to promote.",
} as const;

/** middlegame-blunder after 35… Qxf2: a busy middlegame for the colouring comparison. */
export const BUSY_FEN = "r5k1/2p4p/1p1p2p1/1B1P4/2P1b3/1KQ5/5q2/1R4R1 w - - 0 36";
export const BUSY_LAST: [string, string] = ["f4", "f2"];

/** The fictional player behind the overview mockups. */
export const PLAYER = {
  name: "demo.player (fictional)",
  timeClass: "rapid",
  ratingNow: 1112,
  peak: { rating: 1240, month: "March 2026" },
  change: -128,
  ratedGames: 214,
  before: { accuracy: 76.1, blunders: 1.9, games: 30, span: "January to March 2026" },
  now: { accuracy: 71.4, blunders: 2.6, games: 30, span: "the last 30 rated games" },
  focus: {
    name: "Missing a mate",
    rate: 13.8,
    chances: 94,
    missed: 13,
    target: 10,
    nextGames: 20,
    playedSince: 7,
    chancesSince: 6,
    missedSince: 1,
    peers: 9.6,
  },
  practise: { due: 8, minutes: 10, streakDays: 3, restDayUsed: false },
  areas: [
    { name: "Tactics", note: "Missing a mate is the largest leak", n: 94 },
    { name: "Openings", note: "Early blunders in 31% of games", n: 214 },
    { name: "Endgames", note: "Converting won positions: 9 of 16", n: 16 },
    { name: "Time", note: "Under a minute left in 18% of games", n: 214 },
  ],
} as const;

/** Monthly anchors of the fictional rating line; games are spread between them. */
const ANCHORS: [string, number][] = [
  ["2025-09-01", 1068],
  ["2025-12-01", 1162],
  ["2026-03-11", 1240],
  ["2026-06-01", 1171],
  ["2026-09-01", 1128],
  ["2026-11-18", 1112],
];

/** 214 rated games: the rating before each, along the anchors with a fixed seeded wobble. */
export const TRAJECTORY_POINTS: { id: string; date: string; rating: number }[] = (() => {
  const t0 = Date.parse(`${ANCHORS[0]?.[0]}T12:00:00Z`);
  const t1 = Date.parse(`${ANCHORS[ANCHORS.length - 1]?.[0]}T12:00:00Z`);
  let seed = 7;
  const rand = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 0xffffffff;
  };
  const out: { id: string; date: string; rating: number }[] = [];
  for (let i = 0; i < 214; i++) {
    const t = t0 + ((t1 - t0) * i) / 213;
    let k = 0;
    while (k < ANCHORS.length - 2 && Date.parse(`${ANCHORS[k + 1]?.[0]}T12:00:00Z`) < t) k++;
    const [da, ra] = ANCHORS[k] as [string, number];
    const [db, rb] = ANCHORS[k + 1] as [string, number];
    const ta = Date.parse(`${da}T12:00:00Z`);
    const tb = Date.parse(`${db}T12:00:00Z`);
    const f = Math.min(1, Math.max(0, (t - ta) / (tb - ta)));
    const wobble = i === 0 || i === 213 ? 0 : Math.round((rand() - 0.5) * 14);
    out.push({
      id: `g${i}`,
      date: new Date(t).toISOString(),
      rating: Math.round(ra + (rb - ra) * f) + wobble,
    });
  }
  return out;
})();

/** The facts and version behind the focus recommendation, for "Why this?". */
export const WHY = {
  recommendation: "This week's focus: Missing a mate",
  algorithm: "ranking version 1.4.0",
  algorithmId: "threshold set 7",
  computedAt: "23 November 2026, 07:10",
  facts: [
    {
      label: "Your rate",
      value: "13.8% of mate chances missed (13 of 94)",
      range: "80% likely between 10.2% and 18.3%",
    },
    { label: "Players you face", value: "9.6% at your level", range: "from 1,840 of their games" },
    { label: "Rated games", value: "214 rapid games analysed", range: "since September 2025" },
    {
      label: "Why it ranks first",
      value: "the largest gap that the intervals keep apart",
      range: "next: Early blunders",
    },
  ],
  rules: [
    "A weakness ranks only when at least 30 chances back it.",
    "It ranks above another only when its gap to your opponents is larger and the ranges do not overlap.",
    "The same games, settings and version always give the same order.",
  ],
  inputs: "Stockfish 19 at depth 18, Lichess win-chance scale, games to 22 November 2026",
} as const;
