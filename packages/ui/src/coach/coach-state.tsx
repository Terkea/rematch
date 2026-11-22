"use client";

import {
  BookOpenIcon,
  ClockIcon,
  Icon,
  InfoIcon,
  type PhosphorIcon,
  WarningIcon,
} from "../icons/index.js";
import { cn } from "../lib/index.js";
import { Button } from "../primitives/button.js";
import { ProgressPlaceholder, type ProgressPlaceholderProps } from "../progress/index.js";

// The states a coach surface can be in (ADR 0071). Work in progress is a ProgressPlaceholder in
// the shape of what is coming, never a spinner (B18). Everything else is a quiet note in the
// semantic-state treatments of DESIGN.md: a failure or an exhausted allowance is a warning (ink on
// `well`, a `rule-strong` edge, Phosphor Warning or Clock), and red is never used, because nothing
// here is the player's mistake.

export type CoachStateKind =
  | "building"
  | "writing"
  | "failed"
  | "budget"
  | "unavailable"
  | "computed_only"
  | "empty";

export type CoachStateAction =
  | { label: string; onClick: () => void; href?: never }
  | { label: string; href: string; onClick?: never };

export interface CoachStateProps {
  kind: CoachStateKind;
  title: string;
  body?: string | null;
  action?: CoachStateAction | null;
  /** For `building` and `writing`: the count so far, e.g. `{ done: 2, total: 5, unit: "sections" }`. */
  progress?: ProgressPlaceholderProps["progress"];
  /** The heading element for the title; the host knows the outline. Default `h3`. */
  titleAs?: "h2" | "h3" | "h4" | "p";
  className?: string;
}

const NOTE: Record<
  Exclude<CoachStateKind, "building" | "writing">,
  { icon: PhosphorIcon; tone: "warning" | "info" | "plain"; announce: boolean }
> = {
  failed: { icon: WarningIcon, tone: "warning", announce: true },
  budget: { icon: ClockIcon, tone: "warning", announce: true },
  unavailable: { icon: InfoIcon, tone: "info", announce: true },
  computed_only: { icon: InfoIcon, tone: "info", announce: false },
  empty: { icon: BookOpenIcon, tone: "plain", announce: false },
};

function Action({ action }: { action: CoachStateAction }) {
  if (action.href !== undefined) {
    return (
      <Button asChild variant="outline" size="sm">
        <a href={action.href}>{action.label}</a>
      </Button>
    );
  }
  return (
    <Button variant="outline" size="sm" onClick={action.onClick}>
      {action.label}
    </Button>
  );
}

export function CoachState({
  kind,
  title,
  body,
  action,
  progress,
  titleAs: Title = "h3",
  className,
}: CoachStateProps) {
  if (kind === "building" || kind === "writing") {
    return (
      <div data-coach-state={kind} className={cn("flex flex-col gap-3", className)}>
        <div className="flex max-w-prose flex-col gap-1">
          <Title className="m-0 font-serif text-xl font-semibold text-ink">{title}</Title>
          {body ? <p className="m-0 font-sans text-sm text-ink-soft">{body}</p> : null}
        </div>
        <ProgressPlaceholder
          shape="list"
          verb={kind === "building" ? "Building" : "Writing"}
          {...(progress ? { progress } : {})}
        />
      </div>
    );
  }

  const note = NOTE[kind] ?? NOTE.unavailable;
  return (
    <div
      data-coach-state={kind}
      data-tone={note.tone}
      role={note.announce ? "status" : undefined}
      className={cn(
        "flex gap-3 rounded-field",
        note.tone === "warning" && "border border-rule-strong bg-well p-4",
        note.tone === "info" && "bg-well p-4",
        note.tone === "plain" && "py-2",
        className,
      )}
    >
      <Icon
        icon={note.icon}
        size={20}
        className={cn("mt-0.5 shrink-0", note.tone === "warning" ? "text-ink" : "text-ink-soft")}
      />
      <div className="flex min-w-0 flex-col items-start gap-1">
        <Title className="m-0 font-serif text-lg font-semibold text-ink">{title}</Title>
        {body ? <p className="m-0 max-w-prose font-sans text-sm text-ink-soft">{body}</p> : null}
        {action ? (
          <div className="mt-2">
            <Action action={action} />
          </div>
        ) : null}
      </div>
    </div>
  );
}
