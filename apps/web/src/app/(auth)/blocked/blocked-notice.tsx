import { brand } from "../../../brand";

/**
 * The under-13 page body (ADR-WEB-6), on /blocked and on /sign-in while the block cookie is set:
 * the fact as the one heading, then what happened and what comes next.
 */
export function BlockedNotice() {
  return (
    <div role="status" className="flex flex-col gap-3">
      <h1 id="auth-title" className="font-serif text-3xl font-semibold text-ink">
        You need to be 13 or older to use {/* The brand never breaks at its hyphen. */}
        <span className="whitespace-nowrap">{brand.name}.</span>
      </h1>
      <p className="text-lg text-ink-soft">We've deleted the details you gave us.</p>
      <p className="text-base text-ink-faint">You're welcome back once you turn 13.</p>
    </div>
  );
}
