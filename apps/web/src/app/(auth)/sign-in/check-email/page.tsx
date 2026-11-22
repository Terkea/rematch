import { redirect } from "next/navigation";
import { AuthPage } from "../../auth-page";
import { CheckEmail } from "./check-email";

export const metadata = { title: "Check your email" };

export default async function CheckEmailPage({
  searchParams,
}: {
  searchParams: Promise<{ email?: string | string[] }>;
}) {
  const { email } = await searchParams;
  if (typeof email !== "string" || !email.includes("@")) redirect("/sign-in");
  return (
    <AuthPage diagram>
      <CheckEmail email={email} />
    </AuthPage>
  );
}
