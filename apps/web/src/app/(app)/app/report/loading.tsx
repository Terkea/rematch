import { ProgressPlaceholder } from "@ci/ui/progress";

/** The first load of `/app/report`: the report's shape, never a spinner (B18). */
export default function Loading() {
  return (
    <div className="flex flex-col gap-10">
      <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Your coaching report</h1>
      <ProgressPlaceholder shape="stat" verb="Building" />
      <ProgressPlaceholder shape="list" verb="Building" />
    </div>
  );
}
