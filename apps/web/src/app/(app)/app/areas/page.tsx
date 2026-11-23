import { redirect } from "next/navigation";
import { requireUser } from "../../../../server/auth/session";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { withFilters } from "../../_shell/nav";

export const dynamic = "force-dynamic";

/** `/app/areas` opens the first area, keeping the filters (`account`, `tc`, colour, dates). */
export default async function AreasIndex({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  // It renders nothing, but reads the cursor like every signed-in page (T27): the layout shares it.
  const user = await requireUser();
  await requestEventCursor(user.id);
  const params = await searchParams;
  const q = new URLSearchParams();
  for (const [key, v] of Object.entries(params)) {
    const first = Array.isArray(v) ? v[0] : v;
    if (typeof first === "string" && first !== "") q.set(key, first);
  }
  // The scope, the colour and the dates, like every page link (audit A7), and nothing else.
  redirect(withFilters("/app/areas/openings", q));
}
