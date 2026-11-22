import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { EmptySheet } from "../../../_shell/page-heading";

/** Another user's game, an unknown id and a malformed one all land here, and read the same. */
export default function GameNotFound() {
  return (
    <EmptySheet
      action={
        <Button asChild variant="outline">
          <Link href="/app/games">Games</Link>
        </Button>
      }
    >
      {"We couldn't find that game."}
    </EmptySheet>
  );
}
