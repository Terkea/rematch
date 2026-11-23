import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { BrandMark } from "../components/brand/mark";

export const metadata = { title: "Not found" };

/** Any address that isn't a page: the mark, a line in the coach's voice, and the way back. */
export default function NotFound() {
  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-6 bg-paper px-(--space-gutter) py-16 text-center">
      <BrandMark variant="full" className="h-40 text-ink" />
      <h1 className="font-serif text-3xl font-semibold text-ink">This square is empty</h1>
      <p className="max-w-md text-lg text-ink-soft">
        There's no page at this address. Let's go back and look at your games.
      </p>
      <Button asChild variant="outline">
        <Link href="/">Back to your games</Link>
      </Button>
    </main>
  );
}
