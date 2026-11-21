"use client";

import {
  createContext,
  createElement,
  type ReactNode,
  useContext,
  useEffect,
  useState,
} from "react";
import type { PieceRenderObject } from "react-chessboard";

import {
  animate,
  dur,
  ease,
  motion,
  type PieceType,
  pieceWeight,
  settleFor,
  spring,
  useMotionValue,
  useTransform,
} from "../motion/index.js";
import type { PieceCode, PieceNode } from "./piece-types.js";
import { cburnett } from "./pieces.generated.js";
import type { Square } from "./theme.js";

/** One piece's planned travel, in drawn squares (dx and dy point from the destination back to the origin). */
export interface PiecePlan {
  id: number;
  code: PieceCode;
  type: PieceType;
  dx: number;
  dy: number;
  /** ms before the lift starts (the castling rook follows the king by 70ms, so it lands second) */
  delay: number;
  /** the piece was dropped by drag: it only lands */
  dropped: boolean;
  /** the piece was still in the air from its last move: it travels on from there, already lifted */
  flying?: boolean;
  captured?: { code: PieceCode; dx: number; dy: number; spin: number } | undefined;
}

export interface PieceMotionState {
  plans: ReadonlyMap<Square, PiecePlan>;
  reduced: boolean;
  /** plan ids already played, so a re-mount never replays a move */
  played: Set<number>;
  /** pieces in the air, by square: where each is drawn now, in squares from its own square */
  inflight: Map<Square, () => { dx: number; dy: number }>;
}

export const PieceMotionContext = createContext<PieceMotionState>({
  plans: new Map(),
  reduced: false,
  played: new Set(),
  inflight: new Map(),
});

function renderNode(node: PieceNode, key: number): ReactNode {
  return createElement(
    node.tag,
    { key, ...node.attrs, style: node.style },
    node.children.map((child, i) => renderNode(child, i)),
  );
}

/** One Cburnett piece, painted only with the piece tokens. */
export function PieceGlyph({ code }: { code: PieceCode }) {
  return (
    <svg
      viewBox="0 0 45 45"
      width="100%"
      height="100%"
      data-piece-set="cburnett"
      aria-hidden="true"
      focusable="false"
      style={{ display: "block", fill: "var(--color-piece-dark)", overflow: "visible" }}
    >
      {cburnett[code].map((node, i) => renderNode(node, i))}
    </svg>
  );
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * A piece on its square. When the board planned a move onto this square, it lifts, travels on its
 * own `settle` from the mass table, knights arc over what they jump, and it lands with its squash
 * on `land` (DESIGN.md, "Physical feel"). A capture is knocked off when the mover is within 30% of
 * a square. Under reduced motion the piece appears in place with one opacity cue (M10).
 */
function AnimatedPiece({ code, square }: { code: PieceCode; square?: string | undefined }) {
  const { plans, reduced, played, inflight } = useContext(PieceMotionContext);
  // A move mounts its piece on the destination square, so the plan is read once, at mount: later
  // re-renders (contact, a parent update) never restart or cut the move.
  const [plan] = useState(() => {
    const candidate = square ? plans.get(square as Square) : undefined;
    return candidate && candidate.code === code && !played.has(candidate.id) ? candidate : null;
  });
  const travels = plan !== null && !reduced && !plan.dropped;

  const p = useMotionValue(travels ? 0 : 1);
  const liftAmount = useMotionValue(0);
  const squashX = useMotionValue(1);
  const squashY = useMotionValue(1);
  const opacity = useMotionValue(plan && reduced ? 0 : 1);
  const knocked = useMotionValue(0);
  const [moving, setMoving] = useState(travels);
  const [captureGone, setCaptureGone] = useState(false);

  const dx = plan?.dx ?? 0;
  const dy = plan?.dy ?? 0;
  const knight = plan?.type === "knight";
  const lift = plan ? pieceWeight[plan.type].lift : 1;
  const x = useTransform(p, (v) => `${dx * (1 - v) * 100}%`);
  // Knights arc: y rises 0.2 of a square at mid-travel (DESIGN.md, "Knights arc").
  const y = useTransform(
    p,
    (v) => `${(dy * (1 - v) - (knight ? 0.2 * Math.sin(Math.PI * v) : 0)) * 100}%`,
  );
  const liftY = useTransform(liftAmount, (a) => -4 * a);
  const scale = useTransform([liftAmount, p], ([a, v]) => {
    const base = 1 + (lift - 1) * (a as number);
    return knight && (a as number) > 0 ? base + 0.08 * Math.sin(Math.PI * (v as number)) : base;
  });

  // The captured piece flies 0.6 of a square further in the move's direction (choreography 6).
  const cap = plan?.captured;
  const reach = Math.hypot(dx, dy) || 1;
  const capX = useTransform(knocked, (k) => `${((cap?.dx ?? 0) - (k * 0.6 * dx) / reach) * 100}%`);
  const capY = useTransform(knocked, (k) => `${((cap?.dy ?? 0) - (k * 0.6 * dy) / reach) * 100}%`);
  const capRotate = useTransform(knocked, (k) => k * (cap?.spin ?? 0));
  const capScale = useTransform(knocked, (k) => 1 - 0.1 * k);
  const capOpacity = useTransform(knocked, (k) => 1 - k);

  // Runs once per mount: the plan and the motion values are fixed for the component's life.
  // biome-ignore lint/correctness/useExhaustiveDependencies: mount-only
  useEffect(() => {
    if (!plan) return;
    played.add(plan.id);
    // Start from the origin, also when the effect runs a second time (React's StrictMode).
    if (travels) p.jump(0);
    liftAmount.jump(plan.flying && travels ? 1 : 0);
    if (reduced) {
      const fade = animate(opacity, 1, { duration: dur.quick, ease: ease.out });
      setCaptureGone(true);
      return () => fade.stop();
    }
    let cancelled = false;
    const stops: { stop: () => void }[] = [];
    const weight = pieceWeight[plan.type];
    const land = () => {
      liftAmount.set(0);
      squashX.set(weight.squash.scaleX);
      squashY.set(weight.squash.scaleY);
      stops.push(animate(squashX, 1, spring.land), animate(squashY, 1, spring.land));
    };
    if (plan.dropped) {
      land();
      setCaptureGone(true);
      return () => {
        for (const s of stops) s.stop();
      };
    }
    const distance = Math.hypot(plan.dx, plan.dy);
    const knockAt = Math.max(0, 1 - 0.3 / distance);
    const contactAt = 1 - 0.01 / distance;
    let knockedOff = false;
    let landed = false;
    // While in the air the board can ask where this piece is drawn, so a new move for it starts
    // from there instead of jumping back to this square (M7).
    const where = () => {
      const v = p.get();
      const arc = knight ? 0.2 * Math.sin(Math.PI * v) : 0;
      return { dx: plan.dx * (1 - v), dy: plan.dy * (1 - v) - arc };
    };
    const at = square as Square;
    inflight.set(at, where);
    const leave = () => {
      if (inflight.get(at) === where) inflight.delete(at);
    };
    const touchDown = () => {
      landed = true;
      leave();
      land();
      setMoving(false);
    };
    const unsubscribe = p.on("change", (v) => {
      if (!knockedOff && cap && v >= knockAt) {
        knockedOff = true;
        const knock = animate(knocked, 1, { duration: dur.base, ease: ease.out });
        stops.push(knock);
        knock.then(() => {
          if (!cancelled) setCaptureGone(true);
        });
      }
      if (!landed && v >= contactAt) touchDown();
    });
    (async () => {
      if (plan.delay > 0) await sleep(plan.delay);
      if (cancelled) return;
      if (!plan.flying) {
        const liftAnim = animate(liftAmount, 1, { duration: dur.instant, ease: ease.out });
        stops.push(liftAnim);
        await liftAnim;
      }
      if (cancelled) return;
      const travel = animate(p, 1, settleFor(plan.type));
      stops.push(travel);
      await travel;
      if (!cancelled && !landed) touchDown();
    })();
    return () => {
      cancelled = true;
      leave();
      unsubscribe();
      for (const s of stops) s.stop();
      p.jump(1);
      liftAmount.jump(0);
      squashX.jump(1);
      squashY.jump(1);
    };
  }, []);

  return (
    // The square's height comes from its aspect ratio, so the piece sizes itself the same way.
    <div style={{ position: "relative", width: "100%", aspectRatio: "1 / 1" }}>
      {cap && !captureGone ? (
        <motion.div
          aria-hidden="true"
          data-captured
          style={{
            position: "absolute",
            inset: 0,
            zIndex: 5,
            x: capX,
            y: capY,
            rotate: capRotate,
            scale: capScale,
            opacity: capOpacity,
            pointerEvents: "none",
          }}
        >
          <Scaled>
            <PieceGlyph code={cap.code} />
          </Scaled>
        </motion.div>
      ) : null}
      <motion.div
        data-moving={moving ? "" : undefined}
        style={{
          position: "relative",
          width: "100%",
          height: "100%",
          zIndex: moving ? 10 : 2,
          x,
          y,
          opacity,
          willChange: moving ? "transform" : undefined,
        }}
      >
        <motion.div style={{ width: "100%", height: "100%", y: liftY, scale }}>
          <motion.div
            style={{
              width: "100%",
              height: "100%",
              scaleX: squashX,
              scaleY: squashY,
              transformOrigin: "50% 90%",
            }}
          >
            <Scaled>
              <PieceGlyph code={code} />
            </Scaled>
          </motion.div>
        </motion.div>
      </motion.div>
    </div>
  );
}

function Scaled({ children }: { children: ReactNode }) {
  const inset = "calc((1 - var(--piece-scale, 0.9)) * 50%)";
  return <div style={{ position: "absolute", inset }}>{children}</div>;
}

const CODES: readonly PieceCode[] = [
  "wK",
  "wQ",
  "wR",
  "wB",
  "wN",
  "wP",
  "bK",
  "bQ",
  "bR",
  "bB",
  "bN",
  "bP",
];

/**
 * The set passed to react-chessboard's `pieces` on every board, so its built-in default pieces
 * (unattributed Cburnett) never render (Task 10 ruling). Module-level, so the renderers are stable.
 */
export const boardPieces: PieceRenderObject = Object.fromEntries(
  CODES.map((code) => {
    const render = (props?: { square?: string }) => (
      <AnimatedPiece code={code} square={props?.square} />
    );
    return [code, render];
  }),
);
