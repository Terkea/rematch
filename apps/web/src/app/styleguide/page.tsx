import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { brand } from "../../brand";
import { BrandLockup } from "../../components/brand/lockup";
import { labEnabled } from "../lab/lab-enabled";
import { ForcedStatesStyle } from "./forced-states-style";
import { AnnotationsSection } from "./sections/annotations";
import { AreaScoreSection } from "./sections/area-score";
import { BoardSection } from "./sections/board";
import { BrandSection } from "./sections/brand";
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
import { TrajectoryStyleguideSection } from "./sections/trajectory";
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
        <p className="flex items-center gap-2 whitespace-nowrap text-ink">
          <BrandLockup />
        </p>
        <ThemeToggle />
      </header>
      {/* Every section but the charts skips layout, paint and layerize while it is off screen
          (ADR-WEB-20). The page is about 18,000 elements, and each repaint re-layerized all of
          them, so a trend step at 4x throttle cost a whole frame. The charts stay laid out: the
          trend measures its parent on load, and the e2e specs wait for its dots. `auto` remembers
          a section's real height once it has rendered; one viewport is only the first guess. */}
      <main className="flex flex-col gap-16 pb-16 [&>section:not([data-section='charts'])]:[content-visibility:auto] [&>section:not([data-section='charts'])]:[contain-intrinsic-size:auto_100vh]">
        <HeroSection />
        <FoundationsSection />
        <BrandSection />
        <PrimitivesSection />
        <OverlaysSection />
        <BoardSection />
        <AnnotationsSection />
        <EvalBarSection />
        <EvalGraphSection />
        <ReviewSection />
        <StatsSection />
        <ChartsSection />
        <TrajectoryStyleguideSection />
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
