import { RoundPage } from "../../_lib/page-guard";
import { ThemesPage } from "../../_lib/themes-page";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="board-themes"
      variant="a"
      title="A. slate squares"
      rationale="The shipped squares, kept: cool, low contrast, so pieces and marks carry the eye."
    >
      <ThemesPage colouring="slate" letter="A" />
    </RoundPage>
  );
}
