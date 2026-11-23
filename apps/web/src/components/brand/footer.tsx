import Link from "next/link";
import { brand } from "../../brand";
import { BrandMark } from "./mark";

/** The signed-in and licences footer: a small mark, the name, and where the credits are. */
export function AppFooter() {
  return (
    <footer className="border-t border-rule">
      <div className="mx-auto flex max-w-6xl items-center gap-3 px-(--space-gutter) py-6 text-sm text-ink-soft">
        <BrandMark variant="mid" className="h-10 text-ink" />
        <p className="m-0">{brand.name}, chess improver</p>
        <Link
          href="/licenses"
          className="ml-auto rounded-field underline decoration-rule-strong underline-offset-4 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Licences
        </Link>
      </div>
    </footer>
  );
}
