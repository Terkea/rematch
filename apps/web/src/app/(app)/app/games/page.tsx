import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { requireUser } from "../../../../server/auth/session";
import { EmptySheet, PageHeading } from "../../shell/page-heading";

export const dynamic = "force-dynamic";
export const metadata = { title: "Games" };

/** Placeholder until the games list lands (M5a). The route and its nav entry are already final. */
export default async function GamesPage() {
  await requireUser();
  return (
    <div className="flex flex-col gap-8">
      <PageHeading title="Games" lede="Every game you've played, reviewed move by move." />
      <EmptySheet
        action={
          <Button asChild variant="outline">
            <Link href="/app/accounts">Go to Accounts</Link>
          </Button>
        }
      >
        Your games appear here once an account is connected and they're imported.
      </EmptySheet>
    </div>
  );
}
