import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { getSession } from "../server/auth/session";

export const dynamic = "force-dynamic";

/** The front door: Accounts when signed in, otherwise sign-in. */
export default async function HomePage() {
  const current = await getSession(await headers());
  redirect(current ? "/app/accounts" : "/sign-in");
}
