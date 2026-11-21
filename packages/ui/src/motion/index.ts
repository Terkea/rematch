// The app's one entry point to Motion, so every animation uses the same version and these tokens.
export { AnimatePresence, animate, motion, useAnimate } from "motion/react";
export {
  type CubicBezier,
  dampingRatio,
  dur,
  ease,
  overshoot,
  type PieceType,
  type PieceWeight,
  pieceWeight,
  type Spring,
  settleFor,
  spring,
  stagger,
} from "./tokens.js";
export { useReducedMotion } from "./use-reduced-motion.js";
