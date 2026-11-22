import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { getSession } from "../server/auth/session";
import { getDb } from "../server/db";
import { userAccountIds } from "../server/events/user-accounts";

export const dynamic = "force-dynamic";

/**
 * The front door: the overview for a signed-in user with a linked account, Accounts for one with
 * none yet, otherwise sign-in. `/app` itself still sends an unknown age to `/welcome/age`.
 */
export default async function HomePage() {
  const current = await getSession(await headers());
  if (!current) redirect("/sign-in");
  const links = await userAccountIds(getDb(), current.user.id);
  redirect(links.length > 0 ? "/app" : "/app/accounts");
}
