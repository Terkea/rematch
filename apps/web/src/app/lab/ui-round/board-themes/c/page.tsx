import { RoundPage } from "../../_lib/page-guard";
import { ThemesPage } from "../../_lib/themes-page";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="board-themes"
      variant="c"
      title="C. pencil squares"
      rationale="The most contrast between squares, closest to a printed diagram."
    >
      <ThemesPage colouring="pencil" letter="C" />
    </RoundPage>
  );
}
