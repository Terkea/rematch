import { cookies, headers } from "next/headers";
import { redirect } from "next/navigation";
import { getEnv } from "../../../env";
import { AGE_BLOCK_COOKIE } from "../../../server/age";
import { googleProvider } from "../../../server/auth/google";
import { lichessProvider } from "../../../server/auth/lichess";
import { getSession } from "../../../server/auth/session";
import { AuthPage } from "../auth-page";
import { BlockedNotice } from "../blocked/blocked-notice";
import { SignInForm } from "./sign-in-form";

export const dynamic = "force-dynamic";
export const metadata = { title: "Sign in" };

export default async function SignInPage({
  searchParams,
}: {
  searchParams: Promise<{ error?: string | string[] }>;
}) {
  // A browser blocked by the age gate sees the notice, never the form (ADR-WEB-6).
  if ((await cookies()).has(AGE_BLOCK_COOKIE)) {
    return (
      <AuthPage aside="start">
        <BlockedNotice />
      </AuthPage>
    );
  }
  if (await getSession(await headers())) redirect("/app/accounts");

  const env = getEnv();
  const { error } = await searchParams;
  return (
    <AuthPage
      title="Sign in"
      lede="Your own games, marked up the way a strong coach would."
      aside="mistake"
    >
      <SignInForm
        google={Boolean(googleProvider(env).google)}
        lichess={lichessProvider(env) !== null}
        {...(typeof error === "string" ? { error } : {})}
      />
      <p className="text-sm text-ink-faint">No password. We email you a link that signs you in.</p>
    </AuthPage>
  );
}
