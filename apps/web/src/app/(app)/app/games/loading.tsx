import { PageHeading } from "../../_shell/page-heading";
import { GamesSkeleton } from "./_explorer/skeleton";

/** The first load of `/app/games`: the list's shape, never a spinner (B18). */
export default function Loading() {
  return (
    <div className="flex flex-col gap-6 md:gap-8">
      <PageHeading title="Games" />
      <GamesSkeleton />
    </div>
  );
}
