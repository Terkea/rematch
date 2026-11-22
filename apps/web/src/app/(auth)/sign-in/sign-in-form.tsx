"use client";

import { Icon, WarningCircleIcon } from "@ci/ui/icons";
import { Field, Input } from "@ci/ui/primitives";
import { type FormEvent, useState } from "react";
import { signIn } from "../../../lib/auth-client";
import { PendingButton } from "../../ui/pending-button";
import { RATE_LIMITED, SEND_FAILED, signInErrorMessage } from "./messages";

export interface SignInFormProps {
  /** Google sign-in is configured (both credentials set). */
  google: boolean;
  /** Lichess sign-in is configured (deferred by ADR 0040 until Task 18). */
  lichess: boolean;
  /** The `?error=` code the page was opened with. */
  error?: string;
  navigate?: (href: string) => void;
}

const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** Magic link first, then the configured OAuth providers. Every action shows its own pending state. */
export function SignInForm({
  google,
  lichess,
  error,
  navigate = (href) => window.location.assign(href),
}: SignInFormProps) {
  const [email, setEmail] = useState("");
  const [fieldError, setFieldError] = useState<string>();
  const [formError, setFormError] = useState(signInErrorMessage(error));
  const [pending, setPending] = useState<"email" | "lichess" | "google" | null>(null);

  async function sendLink(event: FormEvent) {
    event.preventDefault();
    if (pending) return;
    const address = email.trim();
    if (!EMAIL.test(address)) {
      setFieldError("Enter your email address, like name@example.com.");
      return;
    }
    setFieldError(undefined);
    setFormError(null);
    setPending("email");
    try {
      const result = await signIn.magicLink({
        email: address,
        callbackURL: "/",
        newUserCallbackURL: "/welcome/age",
        errorCallbackURL: "/sign-in",
      });
      if (result.error) {
        // A blocked browser (age gate) is answered 403: reload to show the notice.
        if (result.error.status === 403) return navigate("/sign-in");
        setFormError(result.error.status === 429 ? RATE_LIMITED : SEND_FAILED);
        setPending(null);
        return;
      }
      // The address goes to the next page in an HttpOnly cookie, never in the URL (review I1).
      navigate("/sign-in/check-email");
    } catch {
      setFormError(SEND_FAILED);
      setPending(null);
    }
  }

  async function social(provider: "lichess" | "google") {
    if (pending) return;
    setPending(provider);
    setFormError(null);
    try {
      await signIn.social({
        provider,
        callbackURL: "/",
        newUserCallbackURL: "/welcome/age",
        errorCallbackURL: "/sign-in",
      });
    } catch {
      setFormError(SEND_FAILED);
      setPending(null);
    }
  }

  return (
    <div className="flex flex-col gap-6">
      {formError ? (
        <p
          role="alert"
          className="flex items-start gap-2 rounded-field bg-well px-4 py-3 text-sm text-pen-text"
        >
          <Icon icon={WarningCircleIcon} className="mt-px shrink-0" />
          {formError}
        </p>
      ) : null}
      <form method="post" onSubmit={sendLink} noValidate className="flex flex-col gap-4">
        <Field label="Email" error={fieldError}>
          <Input
            type="email"
            name="email"
            autoComplete="email"
            inputMode="email"
            spellCheck={false}
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <PendingButton
          type="submit"
          className="w-full"
          pending={pending === "email"}
          pendingLabel="Sending link"
        >
          Email me a sign-in link
        </PendingButton>
      </form>
      {lichess || google ? (
        <div className="flex flex-col gap-3">
          {lichess ? (
            <PendingButton
              type="button"
              variant="outline"
              className="w-full"
              pending={pending === "lichess"}
              pendingLabel="Opening Lichess"
              onClick={() => social("lichess")}
            >
              Continue with Lichess
            </PendingButton>
          ) : null}
          {google ? (
            <PendingButton
              type="button"
              variant="outline"
              className="w-full"
              pending={pending === "google"}
              pendingLabel="Opening Google"
              onClick={() => social("google")}
            >
              Continue with Google
            </PendingButton>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
