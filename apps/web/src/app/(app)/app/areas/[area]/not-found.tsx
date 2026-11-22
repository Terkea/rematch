import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { EmptySheet } from "../../../_shell/page-heading";

/** `/app/areas/<anything else>`: the seven areas are the only pages here. */
export default function AreaNotFound() {
  return (
    <EmptySheet
      action={
        <Button asChild variant="outline">
          <Link href="/app/areas/openings">Openings</Link>
        </Button>
      }
    >
      {"There's no skill area by that name. The seven areas start with your openings."}
    </EmptySheet>
  );
}
