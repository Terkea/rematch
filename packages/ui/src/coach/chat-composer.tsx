"use client";

import { type FormEvent, type KeyboardEvent, useId, useRef, useState } from "react";
import { ClockIcon, Icon } from "../icons/index.js";
import { cn } from "../lib/index.js";
import { Button } from "../primitives/button.js";
import { Kbd } from "../primitives/kbd.js";

// The question box (ADR 0071). Enter sends and Shift+Enter breaks the line; an empty or over-long
// question cannot be sent. Length is counted in code points, as the server counts it
// (`post_user_message`), and the count appears once 200 characters are left. Going over is a form
// error, red's second meaning (DESIGN.md, "Semantic states").

export interface ChatComposerProps {
  maxChars?: number;
  disabled?: boolean;
  /** Shown in place of the send button while disabled, e.g. when the week's questions are used. */
  disabledReason?: string | null;
  /** Return false (or a promise of false) to keep the text, for a refused post. */
  onSubmit: (text: string) => unknown;
  placeholder: string;
  /** The text box's accessible name. */
  label?: string;
  className?: string;
}

/** The count shows from this many characters left. */
export const COUNT_FROM = 200;

const nf = new Intl.NumberFormat("en-GB");

/** Code points, as the server counts them. */
export const charCount = (text: string) => [...text].length;

/** "1,950 characters left", "1 character left", "1 character over". */
export function countText(left: number): string {
  const n = Math.abs(left);
  const unit = n === 1 ? "character" : "characters";
  return left < 0 ? `${nf.format(n)} ${unit} over` : `${nf.format(n)} ${unit} left`;
}

export function ChatComposer({
  maxChars = 2000,
  disabled = false,
  disabledReason,
  onSubmit,
  placeholder,
  label = "Your question",
  className,
}: ChatComposerProps) {
  const id = useId();
  const [value, setValue] = useState("");
  const [sending, setSending] = useState(false);
  const busy = useRef(false);

  const left = maxChars - charCount(value);
  const over = left < 0;
  const blank = value.trim() === "";
  const canSend = !disabled && !sending && !blank && !over;
  const showCount = left <= COUNT_FROM;
  const reason = disabled && disabledReason?.trim() ? disabledReason : null;

  async function send() {
    if (!canSend || busy.current) return;
    busy.current = true;
    setSending(true);
    const text = value;
    try {
      const result = await onSubmit(text);
      if (result !== false) setValue((now) => (now === text ? "" : now));
    } finally {
      busy.current = false;
      setSending(false);
    }
  }

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
    e.preventDefault();
    void send();
  }

  function onFormSubmit(e: FormEvent) {
    e.preventDefault();
    void send();
  }

  const countId = `${id}-count`;
  const reasonId = `${id}-reason`;
  const hintId = `${id}-hint`;

  return (
    <form
      data-chat-composer
      onSubmit={onFormSubmit}
      className={cn("flex flex-col gap-2", className)}
    >
      <label htmlFor={`${id}-box`} className="sr-only">
        {label}
      </label>
      <textarea
        id={`${id}-box`}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        disabled={disabled}
        rows={2}
        aria-invalid={over || undefined}
        aria-describedby={(reason ? [reasonId] : [hintId, showCount ? countId : null])
          .filter(Boolean)
          .join(" ")}
        className={cn(
          "field-sizing-content min-h-20 max-h-60 w-full resize-none overflow-y-auto",
          "rounded-field border border-rule-strong bg-well px-3 py-2.5",
          "font-sans text-base text-ink placeholder:text-ink-faint",
          "transition-colors duration-(--dur-quick) ease-out",
          "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
          "enabled:not-aria-invalid:hover:border-ink-soft aria-invalid:border-pen",
          "disabled:cursor-not-allowed disabled:border-rule disabled:text-ink-faint",
        )}
      />
      {reason ? (
        <p
          id={reasonId}
          className="m-0 flex max-w-prose items-start gap-2 font-sans text-sm text-ink-soft"
        >
          <Icon icon={ClockIcon} size={16} className="mt-0.5 shrink-0" />
          <span>{reason}</span>
        </p>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
          <p
            id={hintId}
            className="m-0 flex items-center gap-1.5 font-sans text-sm text-ink-faint pointer-coarse:sr-only"
          >
            <Kbd>Enter</Kbd> sends, <Kbd>Shift</Kbd>
            <span aria-hidden="true">+</span>
            <span className="sr-only">plus</span>
            <Kbd>Enter</Kbd> adds a line
          </p>
          <div className="ml-auto flex items-center gap-3">
            <p
              id={countId}
              aria-live="polite"
              className={cn(
                "m-0 font-sans text-sm [font-variant-numeric:tabular-nums]",
                over ? "text-pen-text" : "text-ink-faint",
              )}
            >
              {showCount ? countText(left) : null}
            </p>
            <Button type="submit" disabled={!canSend}>
              Send
            </Button>
          </div>
        </div>
      )}
    </form>
  );
}
