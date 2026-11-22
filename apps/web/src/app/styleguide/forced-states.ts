// Shows hover and focus-visible as still states on the styleguide, from the real CSS: every rule
// that uses the pseudo-class is copied with it swapped for `[data-force~="<state>"]`, so an element
// carrying that attribute looks hovered or focused. Nothing is retyped, so the demo can't drift
// from the component (the same technique as Storybook's pseudo-states addon).

export type ForcedPseudo = "hover" | "focus-visible";

/** The CSSOM fields this reads, so tests can pass plain objects. */
export interface RuleLike {
  selectorText?: string;
  style?: { cssText: string };
  cssRules?: ArrayLike<RuleLike>;
  media?: { mediaText: string };
  conditionText?: string;
  name?: string;
}

/** Splits a selector list on its top-level commas only. */
function splitList(selector: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < selector.length; i++) {
    const c = selector[i];
    if (c === "\\") i++;
    else if (c === "(" || c === "[") depth++;
    else if (c === ")" || c === "]") depth--;
    else if (c === "," && depth === 0) {
      parts.push(selector.slice(start, i).trim());
      start = i + 1;
    }
  }
  parts.push(selector.slice(start).trim());
  return parts.filter(Boolean);
}

/**
 * The selector with `:<pseudo>` swapped for the force attribute, keeping only the list parts that
 * use it; null when none does. `:focus` and `:focus-within` never match `focus-visible`.
 */
export function forceSelector(selector: string, pseudo: ForcedPseudo): string | null {
  const source = `:${pseudo}(?![\\w-])`;
  const uses = new RegExp(source);
  const forced = splitList(selector)
    .filter((part) => uses.test(part))
    .map((part) => part.replace(new RegExp(source, "g"), `[data-force~="${pseudo}"]`));
  return forced.length > 0 ? forced.join(", ") : null;
}

/** A nested selector resolved against its parent, as CSS nesting does. */
function resolveNested(child: string, parent: string): string {
  return splitList(child)
    .map((part) =>
      part.includes("&") ? part.replaceAll("&", `:is(${parent})`) : `:is(${parent}) ${part}`,
    )
    .join(", ");
}

const HOVER_MEDIA = /^\(\s*hover\s*:\s*hover\s*\)$/;

function walk(rules: ArrayLike<RuleLike>, parent: string | null, pseudo: ForcedPseudo): string {
  let out = "";
  const emit = (selector: string, declarations: string) => {
    if (!declarations.trim()) return;
    const forced = forceSelector(selector, pseudo);
    if (forced) out += `${forced}{${declarations}}`;
  };
  for (const rule of Array.from(rules)) {
    if (typeof rule.selectorText === "string") {
      const selector = parent ? resolveNested(rule.selectorText, parent) : rule.selectorText;
      emit(selector, rule.style?.cssText ?? "");
      if (rule.cssRules?.length) out += walk(rule.cssRules, selector, pseudo);
    } else if (rule.media && rule.cssRules) {
      const inner = walk(rule.cssRules, parent, pseudo);
      // (hover: hover) is dropped: a forced hover must show on a touch screenshot too.
      if (inner)
        out += HOVER_MEDIA.test(rule.media.mediaText.trim())
          ? inner
          : `@media ${rule.media.mediaText}{${inner}}`;
    } else if (rule.conditionText !== undefined && rule.cssRules) {
      const inner = walk(rule.cssRules, parent, pseudo);
      if (inner) out += `@supports ${rule.conditionText}{${inner}}`;
    } else if (typeof rule.name === "string" && rule.cssRules) {
      const inner = walk(rule.cssRules, parent, pseudo);
      if (inner) out += rule.name ? `@layer ${rule.name}{${inner}}` : inner;
    } else if (parent && rule.style) {
      // Nested declarations apply to the enclosing rule's selector.
      emit(parent, rule.style.cssText);
    }
  }
  return out;
}

/** Every rule in `rules` that uses `:<pseudo>`, rewritten for the force attribute. */
export function forcedStateCss(rules: ArrayLike<RuleLike>, pseudo: ForcedPseudo): string {
  return walk(rules, null, pseudo);
}
