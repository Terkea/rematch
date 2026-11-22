import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { EmptySheet } from "../../../_shell/page-heading";

/** Another user's conversation, a deleted one and an unknown id all land here, and read the same. */
export default function ConversationNotFound() {
  return (
    <EmptySheet
      action={
        <Button asChild variant="outline">
          <Link href="/app/coach">All conversations</Link>
        </Button>
      }
    >
      {"We couldn't find that conversation."}
    </EmptySheet>
  );
}
