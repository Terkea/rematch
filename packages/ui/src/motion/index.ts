// The app's one entry point to Motion, so every animation uses the same version and these tokens.
export {
  AnimatePresence,
  animate,
  type MotionValue,
  motion,
  useAnimate,
  useMotionValue,
  useTransform,
} from "motion/react";
export {
  arrivalTime,
  type CubicBezier,
  dampingRatio,
  dur,
  ease,
  overlay,
  overshoot,
  type PieceType,
  type PieceWeight,
  pieceWeight,
  type Spring,
  settleFor,
  spring,
  springProgress,
  stagger,
} from "./tokens.js";
export { useReducedMotion } from "./use-reduced-motion.js";
