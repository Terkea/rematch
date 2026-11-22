import { brand } from "../../../brand";

/** The under-13 notice (ADR-WEB-6), on /blocked and on /sign-in while the block cookie is set. */
export function BlockedNotice() {
  return (
    <div role="status" className="flex flex-col gap-3 rounded-sheet bg-well p-(--space-panel-pad)">
      <p className="font-serif text-xl font-semibold text-ink">
        You need to be 13 or older to use {brand.name}.
      </p>
      <p className="text-base text-ink-soft">We've deleted the details you gave us.</p>
    </div>
  );
}
