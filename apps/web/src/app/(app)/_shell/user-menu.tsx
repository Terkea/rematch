"use client";

import { Icon, SignOutIcon } from "@ci/ui/icons";
import { toast } from "@ci/ui/overlays";
import { Popover, PopoverContent, PopoverTrigger } from "@ci/ui/primitives";
import { useState } from "react";
import { signOut } from "../../../lib/auth-client";
import { PendingButton } from "../../ui/pending-button";

export interface UserMenuProps {
  email: string;
  /** Where to go after signing out (a full navigation, so no signed-in page stays cached). */
  signOutTo: (href: string) => void;
}

const SIGN_OUT_FAILED = "Couldn't sign out. Try again.";

/**
 * Signs out through Better Auth, then leaves for /sign-in. Better Auth reports a failure as
 * `{ error }` rather than throwing; then the session is still live, so the user stays and is
 * told, instead of bouncing through /sign-in back into the app.
 */
export function useSignOut(signOutTo: (href: string) => void) {
  const [pending, setPending] = useState(false);
  return {
    pending,
    run: async () => {
      if (pending) return;
      setPending(true);
      let failed: boolean;
      try {
        failed = Boolean((await signOut())?.error);
      } catch {
        failed = true;
      }
      if (failed) {
        setPending(false);
        toast.error(SIGN_OUT_FAILED);
        return;
      }
      signOutTo("/sign-in");
    },
  };
}

/** The signed-in address and "Sign out", behind the reader's initial. */
export function UserMenu({ email, signOutTo }: UserMenuProps) {
  const { pending, run } = useSignOut(signOutTo);
  const initial = email.trim().charAt(0).toUpperCase() || "?";
  return (
    <Popover>
      <PopoverTrigger
        aria-label="Account menu"
        className="relative flex size-8 items-center justify-center rounded-pill bg-well font-serif text-base font-semibold text-ink transition-colors duration-(--dur-quick) ease-out hover:bg-sheet focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink pointer-coarse:after:absolute pointer-coarse:after:-inset-1.5"
      >
        <span aria-hidden="true">{initial}</span>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 gap-3" data-surface="sheet">
        <div className="flex flex-col gap-0.5">
          <span className="text-xs text-ink-faint">Signed in as</span>
          <span className="truncate text-sm font-medium text-ink">{email}</span>
        </div>
        <PendingButton
          variant="quiet"
          size="sm"
          className="-mx-2 justify-start"
          pending={pending}
          pendingLabel="Signing out"
          onClick={run}
        >
          <Icon icon={SignOutIcon} />
          Sign out
        </PendingButton>
      </PopoverContent>
    </Popover>
  );
}
