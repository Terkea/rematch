import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { EmptySheet } from "../../_shell/page-heading";

/** A user with no analysed games: the coach has nothing to answer from yet. */
export function NoScope() {
  return (
    <EmptySheet
      action={
        <Button asChild variant="outline">
          <Link href="/app/accounts">Linked accounts</Link>
        </Button>
      }
    >
      Your coach answers from your analysed games, and none are analysed yet. Link an account, then
      ask once its first games are analysed.
    </EmptySheet>
  );
}
