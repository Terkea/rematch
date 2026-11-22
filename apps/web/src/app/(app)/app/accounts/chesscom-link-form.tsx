"use client";

import { Field, Input } from "@ci/ui/primitives";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { PendingButton } from "../../../ui/pending-button";
import { linkErrorMessage, type StartedLink, startChesscomLink } from "./api";

export interface ChesscomLinkFormProps {
  onLinked: (link: StartedLink) => void;
  /** The view's one primary action, unless a pending code's "Check again" holds that place. */
  primary?: boolean;
}

/** A chess.com username, then "Connect chess.com" (Task 19's start route). */
export function ChesscomLinkForm({ onLinked, primary = true }: ChesscomLinkFormProps) {
  const [username, setUsername] = useState("");
  const [error, setError] = useState<string>();
  const start = useMutation({
    mutationFn: startChesscomLink,
    onSuccess: (link) => {
      setUsername("");
      setError(undefined);
      onLinked(link);
    },
    onError: (failure, name) => setError(linkErrorMessage(failure, name)),
  });

  function submit(event: FormEvent) {
    event.preventDefault();
    if (start.isPending) return;
    const name = username.trim();
    if (!name) {
      setError("Enter your chess.com username.");
      return;
    }
    start.mutate(name);
  }

  return (
    <form method="post" onSubmit={submit} noValidate className="flex flex-col gap-3">
      <Field label="chess.com username" error={error}>
        <Input
          name="username"
          autoComplete="off"
          autoCapitalize="none"
          spellCheck={false}
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
      </Field>
      <PendingButton
        type="submit"
        variant={primary ? "primary" : "outline"}
        className="w-full"
        pending={start.isPending}
        pendingLabel="Connecting"
      >
        Connect chess.com
      </PendingButton>
    </form>
  );
}
