import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

/** `/app/areas` opens the first area, keeping the coach scope (`account`, `tc`) and nothing else. */
export default async function AreasIndex({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const params = await searchParams;
  const q = new URLSearchParams();
  for (const key of ["account", "tc"] as const) {
    const v = params[key];
    const first = Array.isArray(v) ? v[0] : v;
    if (typeof first === "string" && first !== "") q.set(key, first);
  }
  const query = q.toString();
  redirect(`/app/areas/openings${query ? `?${query}` : ""}`);
}
