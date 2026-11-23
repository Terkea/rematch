import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { requireUser } from "../server/auth/session";
import { getDb } from "../server/db";
import type { CoachScope } from "../server/profile/types";
import { isSourceKey } from "../server/train/model";
import { practiseCount } from "../server/train/store";

// "Practise these N positions" (ADR 0090): on a weakness the trainer drills, a link into
// `/app/train` with that source only. It reads its own count, so the page only places it.

const count = new Intl.NumberFormat("en-GB");

/** The link for `seriesKey`'s positions, or null for a series the trainer has none of. */
export function practiseLink(
  seriesKey: string,
  scope: Pick<CoachScope, "accountId" | "timeClass">,
  n: number,
): { href: string; label: string } | null {
  if (!isSourceKey(seriesKey) || !Number.isFinite(n) || n <= 0) return null;
  const q = new URLSearchParams({
    account: scope.accountId,
    tc: scope.timeClass,
    source: seriesKey,
  });
  return {
    href: `/app/train?${q.toString()}`,
    label: n === 1 ? "Practise this position" : `Practise these ${count.format(n)} positions`,
  };
}

export async function PractisePositions({
  seriesKey,
  scope,
}: {
  seriesKey: string;
  scope: CoachScope;
}) {
  if (!isSourceKey(seriesKey)) return null;
  const user = await requireUser();
  const link = practiseLink(
    seriesKey,
    scope,
    await practiseCount(getDb(), user.id, scope, seriesKey),
  );
  if (!link) return null;
  return (
    <div>
      <Button asChild variant="outline">
        <Link href={link.href}>{link.label}</Link>
      </Button>
    </div>
  );
}
