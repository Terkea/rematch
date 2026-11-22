import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { AGE_BLOCK_COOKIE } from "../../../../server/age";
import { PENDING_EMAIL_HEADER } from "../../../../server/auth/pending-email";
import { AuthPage } from "../../auth-page";
import { CheckEmail } from "./check-email";

export const dynamic = "force-dynamic";
export const metadata = { title: "Check your email" };

/**
 * Shows the address only as the proxy handed it over from the send's cookie (review I1). It reads
 * nothing from the URL, so no `?email=` text can ever be rendered here.
 */
export default async function CheckEmailPage() {
  if ((await cookies()).has(AGE_BLOCK_COOKIE)) redirect("/sign-in");
  const email = (await headers()).get(PENDING_EMAIL_HEADER);
  return (
    <AuthPage aside="mistake">
      <CheckEmail email={email} />
    </AuthPage>
  );
}
