import { algorithmLink } from "../lib/algorithm";

// "About this recommendation" (ADR 0105): the algorithm version that computed what the page
// recommends, linked to its changelog entry. A quiet line under the content; the full "Why
// this?" disclosure is v0.5's.

const LINK =
  "rounded-field text-ink-soft underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink";

export function AlgorithmNote({ algorithmId }: { algorithmId: string | null | undefined }) {
  const link = algorithmLink(algorithmId);
  return (
    <p data-algorithm-note className="m-0 font-sans text-sm text-ink-faint">
      About this recommendation:{" "}
      {link.id === null ? (
        "computed before algorithm versions were recorded."
      ) : (
        <>
          computed by algorithm {link.version}.{" "}
          <a className={LINK} href={link.href} rel="noreferrer" target="_blank" title={link.id}>
            What changed in each version
          </a>
        </>
      )}
    </p>
  );
}
