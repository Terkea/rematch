import { brand } from "../../brand";
import { BrandMark } from "./mark";

/** "Rematch" in Literata italic, capital R, in the current text colour. */
export function BrandWordmark() {
  return (
    <span className="font-serif-italic text-2xl leading-none font-medium italic">
      {brand.wordmark}
    </span>
  );
}

/**
 * The header lockup: the mid-size mark (drawn for this height, so the boy, his board and his
 * dad's hand still read) beside the wordmark. The "chess improver" tagline is too small to read
 * here; it lives in the README, the social card and the video.
 */
export function BrandLockup() {
  return (
    <>
      <BrandMark variant="mid" className="h-10" />
      <BrandWordmark />
    </>
  );
}
