import { brand } from "../../brand";
import { REMATCH_ICON } from "./icon.generated";

/**
 * The Rematch lockup: the boy with his board (the small-size icon from
 * packages/ui/assets/brand, ink only, never pen red) beside "rematch" in Literata italic. Both
 * draw in the current text colour, so the lockup is ink by day and cream at night. The word is
 * lowercased by CSS only, so screen readers still say "Rematch".
 */
export function BrandLockup() {
  return (
    <>
      <svg
        aria-hidden="true"
        focusable="false"
        viewBox={REMATCH_ICON.viewBox}
        className="size-8 shrink-0"
        // A constant generated from the committed logo (scripts/brand-assets.ts), never user data.
        // biome-ignore lint/security/noDangerouslySetInnerHtml: the generated logo paths
        dangerouslySetInnerHTML={{ __html: REMATCH_ICON.body }}
      />
      <span className="font-serif-italic text-2xl leading-none font-medium lowercase italic">
        {brand.wordmark}
      </span>
    </>
  );
}
