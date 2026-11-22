"use client";

import { useEffect } from "react";
import { type ForcedPseudo, forcedStateCss, type RuleLike } from "./forced-states";

const PSEUDOS: ForcedPseudo[] = ["hover", "focus-visible"];
const MARK = "data-forced-states";

/** Copies the page's own hover and focus-visible rules for `data-force` (see forced-states.ts). */
export function ForcedStatesStyle() {
  useEffect(() => {
    const build = () => {
      let css = "";
      for (const sheet of Array.from(document.styleSheets)) {
        if (sheet.ownerNode instanceof Element && sheet.ownerNode.hasAttribute(MARK)) continue;
        let rules: CSSRuleList;
        try {
          rules = sheet.cssRules;
        } catch {
          continue; // a cross-origin sheet
        }
        for (const pseudo of PSEUDOS) css += forcedStateCss(rules as ArrayLike<RuleLike>, pseudo);
      }
      let el = document.head.querySelector<HTMLStyleElement>(`style[${MARK}]`);
      if (!el) {
        el = document.createElement("style");
        el.setAttribute(MARK, "");
        document.head.append(el);
      }
      if (el.textContent !== css) el.textContent = css;
    };
    build();
    // Dev reloads stylesheets on edit; rebuild when the head changes.
    const observer = new MutationObserver((records) => {
      if (records.some((r) => !(r.target instanceof Element && r.target.hasAttribute(MARK))))
        build();
    });
    observer.observe(document.head, { childList: true });
    return () => {
      observer.disconnect();
      document.head.querySelector(`style[${MARK}]`)?.remove();
    };
  }, []);
  return null;
}
