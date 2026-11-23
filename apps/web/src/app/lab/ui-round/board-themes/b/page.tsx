import { RoundPage } from "../../_lib/page-guard";
import { ThemesPage } from "../../_lib/themes-page";

export const dynamic = "force-dynamic";

export default function Page() {
  return (
    <RoundPage
      surface="board-themes"
      variant="b"
      title="B. paper squares"
      rationale="Lighter and quieter than today, closest to the page; marks stand out most."
    >
      <ThemesPage colouring="paper" letter="B" />
    </RoundPage>
  );
}
