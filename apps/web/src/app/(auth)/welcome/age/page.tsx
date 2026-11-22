import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { getSession } from "../../../../server/auth/session";
import { AuthPage } from "../../auth-page";
import { AgeForm } from "./age-form";

export const dynamic = "force-dynamic";
export const metadata = { title: "Your age" };

/**
 * The age gate's question (ADR-WEB-6). `requireUser` sends a signed-in user here until the birth
 * month is known, so this page reads the session itself instead of calling it.
 */
export default async function AgePage() {
  const current = await getSession(await headers());
  if (!current) redirect("/sign-in");
  if (current.user.birthMonth !== null) redirect("/app/accounts");
  return (
    <AuthPage diagram>
      <AgeForm />
    </AuthPage>
  );
}
