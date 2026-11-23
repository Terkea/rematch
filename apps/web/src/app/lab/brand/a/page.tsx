import { Suspense } from "react";
import { AppFooter } from "../../../../components/brand/footer";
import { BrandMark } from "../../../../components/brand/mark";
import { Header } from "../../../(app)/_shell/header";
import { OverviewEmpty } from "../../../(app)/app/_overview/empty-state";
import { TrainSummary } from "../../../(app)/app/train/train-view";
import { LabFrame } from "../../lab-frame";

// Every placement of the mark in the signed-in app, on fixtures: the header lockup (the real
// Header with a made-up account), the overview's empty state, the trainer's done day, the footer.
const page = { doneToday: 5, rightToday: 4, dueTomorrow: 3 } as Parameters<
  typeof TrainSummary
>[0]["page"];

export default function BrandPlacementsLab() {
  return (
    <LabFrame
      surface="brand"
      variant="a"
      title="The mark in the app"
      rationale="The mid-size mark in the header and footer, a faded one in an empty state, a small one on a finished day; the full drawing only on signed-out pages and the 404."
    >
      <Suspense>
        <Header email="player@example.com" name="Fixture player" />
      </Suspense>
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-10 px-(--space-gutter) py-10">
        <OverviewEmpty state={{ kind: "no-links" }} />
        <TrainSummary page={page} />
        <div className="flex items-end gap-8 text-ink">
          <BrandMark variant="mid" className="h-10" />
          <BrandMark variant="mid" className="h-16" />
          <BrandMark variant="full" className="h-36" />
        </div>
      </main>
      <AppFooter />
    </LabFrame>
  );
}
