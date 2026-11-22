import { Fragment, type ReactNode } from "react";
import { cn } from "../lib/index.js";
import type { Segment } from "./types.js";

// Model text is only ever rendered from stored segments (ADR 0060, ADR 0071): text runs as text,
// moves and lines as notation, numbers as the server's display string, dates in UTC at day
// precision. A kind this build does not know renders nothing, so a newer server never breaks an
// older page.

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "2026-11-12" or an ISO instant to `{ iso: "2026-11-12", text: "12 Nov 2026" }` in UTC. */
export function dayOf(value: unknown): { iso: string; text: string } | null {
  if (typeof value !== "string") return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(value);
  if (!m) return null;
  const date = value.length > 10 ? new Date(value) : new Date(`${m[0]}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return null;
  const y = date.getUTCFullYear();
  const mo = date.getUTCMonth();
  const d = date.getUTCDate();
  const iso = `${y}-${String(mo + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
  return { iso, text: `${d} ${MONTHS[mo]} ${y}` };
}

/** The number as the server formatted it; without a display string, the value in en-GB. */
export function numText(s: { display?: unknown; value?: unknown }): string | null {
  if (typeof s.display === "string" && s.display.trim() !== "") return s.display;
  if (typeof s.value === "number" && Number.isFinite(s.value))
    return s.value.toLocaleString("en-GB");
  return null;
}

/** Notation in running text (DESIGN.md, "Type"): Literata 600, lining tabular figures. */
export const notationClass =
  "font-serif font-semibold text-ink [font-variant-numeric:lining-nums_tabular-nums]";

const linkClass = cn(
  "rounded-field underline decoration-rule-strong decoration-1 underline-offset-4",
  "transition-colors duration-(--dur-quick) ease-out hover:decoration-ink",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
);

const str = (v: unknown): string | null => (typeof v === "string" && v !== "" ? v : null);

/** The solid head of a good arrow, inline: the shape that says "better move" besides the colour. */
function GoodHead() {
  return (
    <svg
      viewBox="0 0 10 10"
      aria-hidden="true"
      focusable="false"
      className="mr-0.5 inline-block size-2.5 fill-good align-baseline"
    >
      <path d="M1 1.5 L9 5 L1 8.5 Z" />
    </svg>
  );
}

export interface SegmentTextProps {
  segments: readonly Segment[];
  /** A URL for a move, best move, engine line or opening line, or null to leave it unlinked. */
  hrefFor?: ((s: Segment) => string | null) | undefined;
  as?: "p" | "span";
  className?: string | undefined;
}

function linked(node: ReactNode, s: Segment, hrefFor: SegmentTextProps["hrefFor"]): ReactNode {
  let href: string | null = null;
  try {
    href = hrefFor ? hrefFor(s) : null;
  } catch {
    href = null;
  }
  return href ? (
    <a href={href} className={linkClass}>
      {node}
    </a>
  ) : (
    node
  );
}

function renderSegment(s: Segment, hrefFor: SegmentTextProps["hrefFor"]): ReactNode {
  switch (s.kind) {
    case "text":
      return str(s.text);
    case "move":
    case "pv": {
      const san = str(s.san);
      if (!san) return null;
      return linked(
        <span data-segment={s.kind} className={notationClass}>
          {san}
        </span>,
        s,
        hrefFor,
      );
    }
    case "best": {
      const san = str(s.san);
      if (!san) return null;
      return linked(
        <span data-segment="best" data-mark="good" className={cn(notationClass, "text-good-text")}>
          <GoodHead />
          <span className="sr-only">better move </span>
          {san}
        </span>,
        s,
        hrefFor,
      );
    }
    case "line": {
      const san = str(s.san);
      if (!san) return null;
      return linked(
        <span data-segment="line" className={notationClass}>
          {san}
        </span>,
        s,
        hrefFor,
      );
    }
    case "num": {
      const text = numText(s);
      return text ? (
        <span data-segment="num" className="[font-variant-numeric:lining-nums_tabular-nums]">
          {text}
        </span>
      ) : null;
    }
    case "date": {
      const day = dayOf(s.date);
      return day ? (
        <time data-segment="date" dateTime={day.iso}>
          {day.text}
        </time>
      ) : null;
    }
    case "square": {
      const square = str(s.square);
      return square ? (
        <span data-segment="square" className={notationClass}>
          {square}
        </span>
      ) : null;
    }
    case "eng": {
      const text = str(s.text);
      if (!text) return null;
      return (
        <span
          role="note"
          aria-label={`Engine check: ${text}`}
          data-segment="eng"
          data-mark="pencil"
          className="whitespace-nowrap"
        >
          <span
            className={cn(
              notationClass,
              "underline decoration-pencil decoration-dashed decoration-2 underline-offset-4",
            )}
          >
            {text}
          </span>
          <span className="ml-1 font-sans text-sm font-medium text-ink-faint">engine check</span>
        </span>
      );
    }
    default:
      return null;
  }
}

/** Stored narrative segments as text, notation and dates. Unknown kinds render nothing. */
export function SegmentText({ segments, hrefFor, as = "p", className }: SegmentTextProps) {
  const Tag = as;
  const list = Array.isArray(segments) ? segments : [];
  return (
    <Tag data-segment-text className={cn(as === "p" && "m-0 whitespace-pre-line", className)}>
      {list.map((s, i) => {
        if (!s || typeof s !== "object") return null;
        // biome-ignore lint/suspicious/noArrayIndexKey: segments are a fixed, ordered run of text.
        return <Fragment key={i}>{renderSegment(s, hrefFor)}</Fragment>;
      })}
    </Tag>
  );
}
