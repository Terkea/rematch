import { brand } from "../../brand";

/**
 * The Rematch wordmark for headers: "Rematch" in Literata italic, capital R, in the current text
 * colour (ink by day, cream at night). At header height the boy with his board is a scribble and
 * the favicon icon reads as clip-art, so the header carries the word alone; the drawing and the
 * "chess improver" tagline live in the hero, the README and the social card.
 */
export function BrandLockup() {
  return (
    <span className="font-serif-italic text-2xl leading-none font-medium italic">
      {brand.wordmark}
    </span>
  );
}
