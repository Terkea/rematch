// Motion tokens from packages/ui/DESIGN.md ("Motion"), mirrored from tokens.css. Durations and
// staggers are in seconds, as Motion takes them. motion.test.ts checks every value against both
// DESIGN.md and tokens.css (ADR-WEB-7).

export const dur = {
  /** 90ms: press feedback, each arrowhead stroke, piece lift */
  instant: 0.09,
  /** 160ms: menus, toggles, hover previews, board cross-fades, retracting marks */
  quick: 0.16,
  /** 240ms: panels, drawers, tab content, a captured piece flying off */
  base: 0.24,
  /** 240ms: one small stroke (graph marker rings, scoresheet rings, condensed redraws) */
  drawSmall: 0.24,
  /** 420ms: one board stroke (ring or arrow shaft) */
  draw: 0.42,
  /** 600ms: the eval graph drawing across */
  reveal: 0.6,
  /** 520ms: a full page turn between routes */
  turn: 0.52,
  /** 180ms: one replayed move in the onboarding stream */
  stream: 0.18,
  /** 900ms: an ink-soak progress segment */
  soak: 0.9,
} as const;

export type CubicBezier = readonly [number, number, number, number];

export const ease = {
  /** UI responses, lift */
  out: [0.2, 0.7, 0.2, 1],
  /** pen strokes: a slow start, a committed sweep and a soft stop */
  pen: [0.6, 0.05, 0.3, 1],
  /** the outgoing page of a page turn */
  turn: [0.7, 0, 0.2, 1],
  /** progress fills soaking in */
  soak: [0.3, 0, 0.1, 1],
} as const satisfies Record<string, CubicBezier>;

export const stagger = {
  /** 40ms: scoresheet lines annotating, list rows */
  tight: 0.04,
  /** 80ms: report sections, critical-moment rings on the graph */
  base: 0.08,
  /** 120ms: ring to arrow within one moment, and nothing else */
  pen: 0.12,
} as const;

export interface Spring {
  readonly type: "spring";
  readonly stiffness: number;
  readonly damping: number;
  readonly mass?: number;
}

const preset = (stiffness: number, damping: number): Spring => ({
  type: "spring",
  stiffness,
  damping,
});

export const spring = {
  /** ζ 0.92: piece travel between squares (use settleFor(piece) for its mass) */
  settle: preset(520, 42),
  /** ζ 0.42, 24% overshoot: a piece's squash and y on arrival, badges landing, streak ticks */
  land: preset(700, 22),
  /** ζ 0.98: stroke progress that must follow input mid-draw */
  pen: preset(150, 24),
  /** ζ 0.93: eval bar and graph cursor for changes below 0.3 win chance */
  swing: preset(260, 30),
  /** ζ 0.62, 8% overshoot: eval bar for changes of 0.3 win chance or more */
  swingBig: preset(260, 20),
  /** ζ 0.98: the incoming page of a page turn, layout morphs, shared-element fallback */
  page: preset(300, 34),
  /** ζ 0.75: a dragged piece's tilt returning to upright */
  tilt: preset(400, 30),
} as const satisfies Record<string, Spring>;

export type PieceType = "pawn" | "knight" | "bishop" | "rook" | "queen" | "king";

export interface PieceWeight {
  readonly mass: number;
  /** uniform scale at the top of the lift */
  readonly lift: number;
  /** the squash set at contact, sprung back to 1 on `land` (origin 50% 90%) */
  readonly squash: { readonly scaleX: number; readonly scaleY: number };
}

const knightOrBishop: PieceWeight = { mass: 1, lift: 1.05, squash: { scaleX: 1.02, scaleY: 0.97 } };
const queenOrKing: PieceWeight = { mass: 1.25, lift: 1.07, squash: { scaleX: 1.03, scaleY: 0.96 } };

/** Mass makes weight visible: heavier pieces travel slower, settle looser and land harder. */
export const pieceWeight: Readonly<Record<PieceType, PieceWeight>> = {
  pawn: { mass: 0.8, lift: 1.04, squash: { scaleX: 1.015, scaleY: 0.98 } },
  knight: knightOrBishop,
  bishop: knightOrBishop,
  rook: { mass: 1.15, lift: 1.06, squash: { scaleX: 1.025, scaleY: 0.965 } },
  queen: queenOrKing,
  king: queenOrKing,
};

/** The `settle` spring carrying this piece's mass. */
export function settleFor(piece: PieceType): Spring {
  return { ...spring.settle, mass: pieceWeight[piece].mass };
}

/** ζ = damping / (2 × √(stiffness × mass)). Below 1 the spring overshoots. */
export function dampingRatio({ stiffness, damping, mass = 1 }: Spring): number {
  return damping / (2 * Math.sqrt(stiffness * mass));
}

/** The first overshoot as a fraction of the distance travelled: exp(−ζπ / √(1 − ζ²)). */
export function overshoot(s: Spring): number {
  const zeta = dampingRatio(s);
  if (zeta >= 1) return 0;
  return Math.exp((-zeta * Math.PI) / Math.sqrt(1 - zeta * zeta));
}
