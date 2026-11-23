"use client";

import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { BrandMark } from "../components/brand/mark";

/**
 * A page that failed to render: the mark, what happened in plain words, and two ways on. The
 * error's own message stays out of the page (it can carry internals); the server log has it.
 */
export default function ErrorPage({
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-6 bg-paper px-(--space-gutter) py-16 text-center">
      <BrandMark variant="full" className="h-40 text-ink" />
      <h1 className="font-serif text-3xl font-semibold text-ink">Something went wrong</h1>
      <p className="max-w-md text-lg text-ink-soft">
        That one's on us, not you. Try again, and if it keeps happening, go back to your games.
      </p>
      <div className="flex flex-wrap justify-center gap-3">
        <Button onClick={reset}>Try again</Button>
        <Button asChild variant="outline">
          <Link href="/">Back to your games</Link>
        </Button>
      </div>
    </main>
  );
}
