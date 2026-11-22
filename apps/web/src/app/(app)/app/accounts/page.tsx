import { getEnv } from "../../../../env";
import { listAccounts } from "../../../../server/accounts/list";
import { lichessProvider } from "../../../../server/auth/lichess";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { AccountsView } from "./accounts-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Accounts" };

export default async function AccountsPage() {
  const user = await requireUser();
  const env = getEnv();
  const accounts = await listAccounts(getDb(), user.id);
  // While chess.com linking is off, its links are hidden too (as GET /api/accounts does).
  const visible = env.CHESSCOM_ENABLED
    ? accounts
    : accounts.filter((account) => account.platform !== "chesscom");
  return (
    <AccountsView
      initialAccounts={visible}
      chesscom={env.CHESSCOM_ENABLED}
      lichess={lichessProvider(env) !== null}
    />
  );
}
