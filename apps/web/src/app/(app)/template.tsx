"use client";

import { motion, spring, useReducedMotion } from "@ci/ui/motion";
import { type ReactNode, useEffect, useState } from "react";
import { morphPending } from "../../components/shared-morph";

// Set after the first page has mounted, so only client navigations turn the page: the first
// paint is the server render, never a hidden page waiting for JavaScript.
let navigated = false;

const turn = { rotateY: 14, x: 24, opacity: 0 };

/**
 * The page turn's incoming sheet (DESIGN.md, choreography 5): it swings in from its right edge
 * with `rotateY(14deg) translateX(24px)` on the `page` spring, opacity over its first 160ms. The
 * header stays still. Under reduced motion it is an instant swap (M10).
 */
export default function AppTemplate({ children }: { children: ReactNode }) {
  const reduced = useReducedMotion();
  // Decided once per mount (a navigation remounts the template), so a re-render never re-turns.
  // A shared-element morph replaces the turn (choreography 4).
  const [animate] = useState(() => navigated && !reduced && !morphPending());
  useEffect(() => {
    navigated = true;
  }, []);
  return (
    <div className="flex flex-1 flex-col" style={{ perspective: 1600 }}>
      <motion.div
        className="flex flex-1 flex-col"
        style={{ transformOrigin: "right center", backfaceVisibility: "hidden" }}
        initial={animate ? turn : false}
        animate={{ rotateY: 0, x: 0, opacity: 1 }}
        transition={{ ...spring.page, opacity: { duration: 0.16, ease: "linear" } }}
      >
        {children}
      </motion.div>
    </div>
  );
}
