import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { brand } from "../../brand";
import { labEnabled } from "../lab/lab-enabled";
import { ForcedStatesStyle } from "./forced-states-style";
import { AnnotationsSection } from "./sections/annotations";
import { AreaScoreSection } from "./sections/area-score";
import { BoardSection } from "./sections/board";
import { ChartsSection } from "./sections/charts";
import { CoachSection } from "./sections/coach";
import { EvalBarSection } from "./sections/eval-bar";
import { EvalGraphSection } from "./sections/eval-graph";
import { FocusSection } from "./sections/focus";
import { FoundationsSection } from "./sections/foundations";
import { HeroSection } from "./sections/hero";
import { OverlaysSection } from "./sections/overlays";
import { PositionExplorerSection } from "./sections/position-explorer";
import { PrimitivesSection } from "./sections/primitives";
import { ProfileSection } from "./sections/profile";
import { ProgressSection } from "./sections/progress";
import { ReviewSection } from "./sections/review";
import { SoundSection } from "./sections/sound";
import { StatsSection } from "./sections/stats";
import { TimelineSection } from "./sections/timeline";
import { ThemeToggle } from "./theme-toggle";

export const metadata: Metadata = { title: `Styleguide, ${brand.name}` };

// Checked per request, so a production build without LAB_ENABLED=1 serves a 404.
export const dynamic = "force-dynamic";

export default function StyleguidePage() {
  if (!labEnabled(process.env)) notFound();

  return (
    <div className="mx-auto max-w-7xl px-(--space-gutter)">
      <ForcedStatesStyle />
      <header className="flex items-center justify-between gap-4 py-5">
        <p className="font-serif text-lg font-semibold whitespace-nowrap">{brand.wordmark}</p>
        <ThemeToggle />
      </header>
      <main className="flex flex-col gap-16 pb-16">
        <HeroSection />
        <FoundationsSection />
        <PrimitivesSection />
        <OverlaysSection />
        <BoardSection />
        <AnnotationsSection />
        <EvalBarSection />
        <EvalGraphSection />
        <ReviewSection />
        <StatsSection />
        <ChartsSection />
        <AreaScoreSection />
        <TimelineSection />
        <ProfileSection />
        <ProgressSection />
        <FocusSection />
        <CoachSection />
        <PositionExplorerSection />
        <SoundSection />
      </main>
    </div>
  );
}
