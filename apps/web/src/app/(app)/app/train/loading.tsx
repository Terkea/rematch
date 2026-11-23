import { PageHeading } from "../../_shell/page-heading";

/**
 * The first load of `/app/train`: the board's square in a well and the prompt's lines, never a
 * spinner (B18). The line above them says what is happening.
 */
export default function Loading() {
  return (
    <div className="flex flex-col gap-12">
      <PageHeading title="Train" />
      <div role="status" className="flex flex-col gap-6">
        <p className="font-serif text-base text-ink-soft">Choosing today's positions</p>
        <div aria-hidden="true" className="flex flex-col gap-8 lg:flex-row lg:items-start">
          <span className="block size-(--board-size) rounded-board bg-well" />
          <div className="flex flex-1 flex-col gap-4">
            <span className="h-6 w-56 max-w-full rounded-pill bg-well" />
            <span className="h-4 w-72 max-w-full rounded-pill bg-well" />
            <span className="h-10 w-40 rounded-field bg-well" />
          </div>
        </div>
      </div>
    </div>
  );
}
