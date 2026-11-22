import { brand } from "../../../brand";

// The sign-in page's messages for `?error=` (Task 21 copy; not_invited and email_not_verified
// from ADR-WEB-3). They say what happened and what to do, never whether an account exists.
const messages: Record<string, string> = {
  link_expired: "This link has expired. Request a new one.",
  lichess_denied: "Lichess sign-in was cancelled. You can try again or use your email.",
  lichess_taken: "This Lichess account is connected to another user.",
  lichess_failed: "Lichess sign-in didn't finish. Try again or use your email.",
  account_not_linked:
    "That email already has an account. Sign in with your email link, then connect Lichess from Accounts.",
  not_invited: `This email can't sign in yet. ${brand.name} is invite-only for now.`,
  email_not_verified:
    "Google hasn't confirmed that email address. Sign in with your email link instead.",
};

export function signInErrorMessage(code: string | undefined | null): string | null {
  if (!code) return null;
  return messages[code] ?? null;
}

export const RATE_LIMITED = "Too many sign-in links were requested. Try again in 10 minutes.";
export const SEND_FAILED = "The sign-in link couldn't be sent. Try again.";
