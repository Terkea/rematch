"use client";

import { toast } from "@ci/ui/overlays";
import { Button, Select } from "@ci/ui/primitives";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { DEFAULT_GAMES_N, GAMES_N_CHOICES } from "../../server/focus/goal";
import type { TimeClass } from "../../server/games/types";
import { type FocusTarget, MakeFocusButton } from "./make-focus-button";

const NETWORK = "Couldn't reach the server. Check your connection and try again.";

function lengthOptions(current: number) {
  const values = [...new Set([...GAMES_N_CHOICES, current])].sort((a, b) => a - b);
  return values.map((n) => ({ value: String(n), label: `${n} games` }));
}

async function send(method: "PATCH" | "DELETE", body: unknown): Promise<boolean> {
  try {
    const res = await fetch("/api/focus", {
      method,
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (res.ok) return true;
    toast.error(
      res.status === 404
        ? "There's no running focus to change. Reload the page."
        : "Something went wrong. Try again.",
    );
    return false;
  } catch {
    toast.error(NETWORK);
    return false;
  }
}

/** The running focus's controls: its length and "Clear focus". */
export function ActiveFocusControls({
  timeClass,
  gamesN,
}: {
  timeClass: TimeClass;
  gamesN: number;
}) {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const run = async (method: "PATCH" | "DELETE", body: unknown, done: string) => {
    if (pending) return;
    setPending(true);
    const ok = await send(method, body);
    setPending(false);
    if (ok) {
      toast(done);
      router.refresh();
    }
  };
  return (
    <div className="flex flex-wrap items-end gap-3">
      <Select
        label="Length"
        value={String(gamesN)}
        options={lengthOptions(gamesN)}
        className="w-36"
        onValueChange={(v) => {
          const n = Number(v);
          if (n !== gamesN) void run("PATCH", { timeClass, gamesN: n }, "Focus length changed");
        }}
      />
      <Button
        variant="quiet"
        size="md"
        aria-disabled={pending || undefined}
        onClick={() => void run("DELETE", { timeClass }, "Focus cleared")}
      >
        Clear focus
      </Button>
    </div>
  );
}

/** A suggested focus's controls: its length, then "Make this my focus". */
export function SuggestFocusControls({
  target,
  label = "Make this my focus",
}: {
  target: FocusTarget;
  label?: string;
}) {
  const [gamesN, setGamesN] = useState<number>(DEFAULT_GAMES_N);
  return (
    <div className="flex flex-wrap items-end gap-3">
      <Select
        label="Length"
        value={String(gamesN)}
        options={lengthOptions(gamesN)}
        className="w-36"
        onValueChange={(v) => setGamesN(Number(v))}
      />
      <MakeFocusButton
        target={target}
        current={false}
        gamesN={gamesN}
        label={label}
        variant="primary"
        size="md"
      />
    </div>
  );
}
