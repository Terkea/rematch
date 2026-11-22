import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { requireUser } from "../../../server/auth/session";
import { EmptySheet, PageHeading } from "../_shell/page-heading";

export const dynamic = "force-dynamic";
export const metadata = { title: "Dashboard" };

/** Placeholder until the dashboard lands (M5a). The route and its nav entry are already final. */
export default async function DashboardPage() {
  await requireUser();
  return (
    <div className="flex flex-col gap-8">
      <PageHeading title="Dashboard" lede="Where your games go wrong, and what to practise." />
      <EmptySheet
        action={
          <Button asChild variant="outline">
            <Link href="/app/accounts">Go to Accounts</Link>
          </Button>
        }
      >
        Your dashboard fills in once your games are imported and analysed.
      </EmptySheet>
    </div>
  );
}
