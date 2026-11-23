"use client";

import { toast } from "@ci/ui/overlays";
import { Button } from "@ci/ui/primitives";
import { useRouter } from "next/navigation";
import { useState } from "react";
import type { TimeClass } from "../../server/games/types";
import type { Side } from "../../server/profile/types";

export type FocusTarget = {
  accountId: string;
  timeClass: TimeClass;
  seriesKey: string;
  side: Side;
};

const NETWORK = "Couldn't reach the server. Check your connection and try again.";

/** POST /api/focus, then a toast and a refresh; true when the focus was set. */
export async function postFocus(target: FocusTarget, gamesN: number): Promise<boolean> {
  try {
    const res = await fetch("/api/focus", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ...target, gamesN }),
    });
    if (res.status === 201) {
      toast("Focus set");
      return true;
    }
    toast.error(
      res.status === 422
        ? "This weakness has no rate to bring down yet, so it can't be a focus."
        : res.status === 404
          ? "This weakness isn't measured in these games any more. Reload the page."
          : res.status === 401
            ? "You're signed out. Sign in and try again."
            : "Something went wrong. Try again.",
    );
    return false;
  } catch {
    toast.error(NETWORK);
    return false;
  }
}

/**
 * "Make this my focus" (ADR 0090): makes the weakness the focus of its time class over the next
 * `gamesN` games, replacing a running one. The target is the server's, from the current estimate.
 * When the weakness already is the focus it says so instead.
 */
export function MakeFocusButton({
  target,
  current,
  gamesN = 20,
  label = "Make this my focus",
  variant = "outline",
}: {
  target: FocusTarget;
  current: boolean;
  gamesN?: number;
  label?: string;
  variant?: "primary" | "outline";
}) {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  if (current) {
    return <p className="m-0 text-sm text-ink-soft">This is your focus this week.</p>;
  }
  return (
    <Button
      variant={variant}
      size="sm"
      className="w-fit"
      aria-disabled={pending || undefined}
      onClick={async () => {
        if (pending) return;
        setPending(true);
        const ok = await postFocus(target, gamesN);
        setPending(false);
        if (ok) router.refresh();
      }}
    >
      {label}
    </Button>
  );
}
