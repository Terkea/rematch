"use client";

import { useEffect } from "react";
import { REVEAL_COOKIE } from "./view";

/** Marks the reveal as seen for this browser session (no max-age: it ends with the session). */
export function RevealSeen() {
  useEffect(() => {
    // biome-ignore lint/suspicious/noDocumentCookie: one flag, no store needed
    document.cookie = `${REVEAL_COOKIE}=1; path=/app; SameSite=Lax`;
  }, []);
  return null;
}
