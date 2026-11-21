import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { brand } from "../../brand";
import { AnnotationsSection } from "./sections/annotations";
import { AreaScoreSection } from "./sections/area-score";
import { BoardSection } from "./sections/board";
import { EvalBarSection } from "./sections/eval-bar";
import { EvalGraphSection } from "./sections/eval-graph";
import { FoundationsSection } from "./sections/foundations";
import { OverlaysSection } from "./sections/overlays";
import { PrimitivesSection } from "./sections/primitives";
import { ProgressSection } from "./sections/progress";
import { StatsSection } from "./sections/stats";
import { TimelineSection } from "./sections/timeline";
import { ThemeToggle } from "./theme-toggle";

export const metadata: Metadata = { title: `Styleguide, ${brand.name}` };

// Checked per request, so a production build without LAB_ENABLED=1 serves a 404.
export const dynamic = "force-dynamic";

export default function StyleguidePage() {
  if (process.env.NODE_ENV === "production" && process.env.LAB_ENABLED !== "1") notFound();

  return (
    <div className="mx-auto max-w-7xl px-(--space-gutter)">
      <header className="flex items-center justify-between gap-4 py-5">
        <p className="font-serif text-lg font-semibold whitespace-nowrap">{brand.wordmark}</p>
        <ThemeToggle />
      </header>
      <main className="flex flex-col gap-10 pb-16">
        <FoundationsSection />
        <PrimitivesSection />
        <OverlaysSection />
        <BoardSection />
        <AnnotationsSection />
        <EvalBarSection />
        <EvalGraphSection />
        <StatsSection />
        <AreaScoreSection />
        <TimelineSection />
        <ProgressSection />
      </main>
    </div>
  );
}
