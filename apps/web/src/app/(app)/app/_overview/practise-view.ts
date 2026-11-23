import type { TimeClass } from "../../../../server/games/types";
import {
  addDays,
  type PractiseToday,
  SESSION_SIZE,
  type SourceKey,
} from "../../../../server/train/model";
import { SOURCE_LABEL } from "../train/copy";

// The overview's practise call (ADR 0092), in words: the count due today, done for today with
// the next due day, or a first run. The link is the trainer on the overview's own scope, kept to
// the weekly focus's source when that source has positions.

export type PractiseView =
  | { kind: "due"; action: string; href: string; detail: string }
  | { kind: "done"; line: string; detail: string; href: string }
  | { kind: "first-run"; action: string; href: string; detail: string };

const count = new Intl.NumberFormat("en-GB");
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const positions = (n: number) => `${count.format(n)} ${n === 1 ? "position" : "positions"}`;

function trainHref(
  scope: { accountId: string; timeClass: TimeClass },
  source: SourceKey | null,
): string {
  const q = new URLSearchParams({ account: scope.accountId, tc: scope.timeClass });
  if (source) q.set("source", source);
  return `/app/train?${q.toString()}`;
}

/** "tomorrow", or "on 30 November" (with the year when it is not this one). */
function nextWords(next: string, today: string): string {
  if (next === addDays(today, 1)) return "tomorrow";
  const d = new Date(`${next}T00:00:00Z`);
  const year = next.slice(0, 4) === today.slice(0, 4) ? "" : ` ${d.getUTCFullYear()}`;
  return `on ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}${year}`;
}

export function practiseView(
  p: PractiseToday,
  scope: { accountId: string; timeClass: TimeClass },
  today: string,
): PractiseView {
  const tc = scope.timeClass;
  const what = `your own ${tc} games where you missed a win or a mate, or blundered`;
  switch (p.kind) {
    case "first-run":
      return {
        kind: "first-run",
        action: "Start practising",
        href: trainHref(scope, null),
        detail: `Let's go over the moments in your own ${tc} games where you missed a win or a mate, or blundered. Up to ${SESSION_SIZE} a session, about 10 to 15 minutes.`,
      };
    case "due":
      return {
        kind: "due",
        action: `Practise today: ${positions(p.count)} due`,
        href: trainHref(scope, p.source),
        detail: p.source
          ? `From your focus, ${SOURCE_LABEL[p.source]}, in your ${tc} games.`
          : `Positions from ${what}.`,
      };
    case "done": {
      const done =
        p.doneToday > 0 ? `You went through ${positions(p.doneToday)} today. Well done.` : "";
      const nothing =
        p.next === null
          ? "Nothing else is due yet. Your next analysed games will bring new positions."
          : "";
      return {
        kind: "done",
        line:
          p.next === null
            ? "That's today done."
            : `That's today done. Next due ${nextWords(p.next, today)}.`,
        detail: [done, nothing].filter((s) => s !== "").join(" "),
        href: trainHref(scope, null),
      };
    }
  }
}
