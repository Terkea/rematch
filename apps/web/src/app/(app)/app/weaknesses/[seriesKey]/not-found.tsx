import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { MorphReset } from "../../../../../components/shared-morph";
import { EmptySheet } from "../../../_shell/page-heading";

/** An unknown series key, another user's and a malformed one all land here, and read the same. */
export default function WeaknessNotFound() {
  return (
    <>
      <MorphReset />
      <EmptySheet
        action={
          <Button asChild variant="outline">
            <Link href="/app/weaknesses">All weaknesses</Link>
          </Button>
        }
      >
        {"We couldn't find that weakness."}
      </EmptySheet>
    </>
  );
}
