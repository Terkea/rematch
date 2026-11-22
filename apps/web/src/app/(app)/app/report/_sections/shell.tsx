"use client";

import { AiDisclosure, type AiDisclosureData, CoachState, SegmentText } from "@ci/ui/coach";
import { cn } from "@ci/ui/lib";
import { type CSSProperties, createContext, type ReactNode, useContext } from "react";
import type { NarrativeItem, SectionContent } from "../../../../../server/coaching/types";
import { SectionHead } from "../../_overview/section-head";
import { reviewHref } from "./links";

/** Platform game id to `games.id` for the scope (only the scope's games are in it). */
export const GameIdsContext = createContext<Record<string, string>>({});

/**
 * Where a section's coach notes stand: `ready` (validated content), `dropped` (the validator
 * removed it), `writing` (not written yet while the run builds), `missing` (the run ended without
 * it) or `off` (the AI was off, which the banner already says).
 */
export type SectionStatus = "ready" | "dropped" | "writing" | "missing" | "off";

export function sectionStatus(
  content: SectionContent | null | undefined,
  ctx: { building: boolean; aiOff: boolean },
): SectionStatus {
  if (content) return "ready";
  if (content === null) return "dropped";
  if (ctx.building) return "writing";
  return ctx.aiOff ? "off" : "missing";
}

/** The note a section shows in place of its coach's text, or nothing. */
export function SectionNote({ status }: { status: SectionStatus }) {
  switch (status) {
    case "writing":
      return <CoachState kind="writing" titleAs="p" title="Your coach is writing this section" />;
    case "dropped":
      return (
        <CoachState
          kind="unavailable"
          titleAs="p"
          title="Your coach's notes for this section couldn't be checked, so they're not shown"
        />
      );
    case "missing":
      return (
        <CoachState
          kind="computed_only"
          titleAs="p"
          title="Your coach didn't write this section, so only the numbers are shown"
        />
      );
    default:
      return null;
  }
}

/**
 * One validated coach statement, rendered only from its stored segments (ADR 0060); a move in one
 * of the scope's games links to its review at that ply. Never the raw `text`.
 */
export function CoachText({
  item,
  className,
}: {
  item: NarrativeItem | null | undefined;
  className?: string;
}) {
  const gameIds = useContext(GameIdsContext);
  if (!item || !Array.isArray(item.segments) || item.segments.length === 0) return null;
  return (
    <SegmentText
      segments={item.segments as never}
      hrefFor={(s) => reviewHref(s, gameIds)}
      className={cn("max-w-prose font-serif text-lg leading-relaxed text-ink", className)}
    />
  );
}

/**
 * A report section: the overview's ruled head (it draws on the first view, `order` places it in
 * the reveal) and the compact AI disclosure when the section shows coach text.
 */
export function ReportSection({
  id,
  title,
  order,
  intro,
  disclosure,
  children,
}: {
  id: string;
  title: string;
  order: number;
  intro?: ReactNode;
  disclosure?: AiDisclosureData | null;
  children: ReactNode;
}) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-5">
      <SectionHead id={id} title={title} order={order}>
        {intro}
      </SectionHead>
      <div
        data-reveal-fig=""
        style={{ "--reveal-i": order } as CSSProperties}
        className="flex flex-col gap-5"
      >
        {disclosure ? <AiDisclosure disclosure={disclosure} compact /> : null}
        {children}
      </div>
    </section>
  );
}
