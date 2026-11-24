/** The first load of one weakness: its header and sheet in pencil wells, never a spinner (B18). */
export default function Loading() {
  return (
    <div data-route-loading role="status" className="flex flex-col gap-8">
      <p className="sr-only">Reading this weakness</p>
      <div aria-hidden="true" className="flex flex-col gap-4">
        <span className="h-3 w-28 rounded-pill bg-well" />
        <span className="h-8 w-72 max-w-full rounded-field bg-well" />
        <span className="h-4 w-32 rounded-pill bg-well" />
      </div>
      <div
        aria-hidden="true"
        className="grid grid-cols-1 gap-8 rounded-sheet bg-sheet px-(--space-panel-pad) py-8 md:grid-cols-2"
      >
        <div className="flex flex-col gap-3">
          <span className="h-10 w-24 rounded-field bg-well" />
          <span className="h-3 w-40 rounded-pill bg-well" />
        </div>
        <span className="h-3 w-full self-center rounded-pill bg-well" />
      </div>
    </div>
  );
}
