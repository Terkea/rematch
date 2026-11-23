import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { useId } from "react";
import type { FocusCard as FocusCardModel, Suggestion } from "../../server/focus";
import type { Progress } from "../../server/focus/goal";
import type { TimeClass } from "../../server/games/types";
import type { Side } from "../../server/profile/types";
import { ActiveFocusControls, SuggestFocusControls } from "./focus-controls";
import {
  gamesText,
  goalText,
  outcomeText,
  pctText,
  progressText,
  targetWhyText,
  verdictText,
} from "./focus-text";

// The weekly focus card (ADR 0091): the one thing to work on this week, its goal and whether it
// is working. A plain sheet (no coloured border, B10); the verdict is a pill whose wording
// carries the meaning, never its colour. Exported for the overview and the progress page.

type Scope = { accountId: string; timeClass: TimeClass; side: Side };

const LINK =
  "rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink";

function query(s: Scope): string {
  const q = new URLSearchParams({ account: s.accountId, tc: s.timeClass });
  if (s.side !== "both") q.set("color", s.side);
  return q.toString();
}

const weaknessHref = (seriesKey: string, s: Scope) =>
  `/app/weaknesses/${encodeURIComponent(seriesKey)}?${query(s)}`;

/** The trainer, started on this weakness's own positions (built by the trainer lane). */
export const practiseHref = (seriesKey: string) =>
  `/app/train?source=${encodeURIComponent(seriesKey)}`;

const colourNote = (side: Side) =>
  side === "white" ? " Your White games only." : side === "black" ? " Your Black games only." : "";

function Sheet({ title, children }: { title: string; children: React.ReactNode }) {
  const id = useId();
  return (
    <section
      aria-labelledby={id}
      data-slot="focus"
      data-surface="sheet"
      className="flex max-w-3xl flex-col gap-4 rounded-sheet bg-sheet px-(--space-panel-pad) py-5"
    >
      <h2 id={id} className="m-0 text-base font-medium text-ink-soft">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Name({ label, href }: { label: string; href: string }) {
  return (
    <Link
      href={href}
      className={`-mt-3 self-start font-serif text-xl font-semibold [overflow-wrap:anywhere] ${LINK}`}
    >
      {label}
    </Link>
  );
}

/** The games bar: games played out of N, with the count beside it. */
function GamesBar({ progress }: { progress: Progress }) {
  const { played, of } = progress.games;
  const share = of > 0 ? Math.min(100, Math.max(0, (played / of) * 100)) : 0;
  const text = gamesText(progress);
  return (
    <div className="flex flex-col gap-1.5">
      <div
        role="progressbar"
        aria-label="Games played in this focus"
        aria-valuemin={0}
        aria-valuemax={of}
        aria-valuenow={played}
        aria-valuetext={text}
        className="h-2 overflow-hidden rounded-board border border-rule-strong bg-well"
      >
        <span className="block h-full bg-ink" style={{ width: `${share}%` }} />
      </div>
      <span aria-hidden="true" className="text-sm text-ink-soft [font-variant-numeric:lining-nums]">
        {text}
      </span>
    </div>
  );
}

function Pill({ text, provisional, data }: { text: string; provisional: boolean; data: string }) {
  return (
    <span
      data-verdict={data}
      className={
        provisional
          ? "w-fit rounded-pill border border-dashed border-ink-faint px-2.5 py-0.5 text-sm font-medium text-ink"
          : "w-fit rounded-pill bg-well px-2.5 py-0.5 text-sm font-medium text-ink"
      }
    >
      {text}
    </span>
  );
}

function Practise({ seriesKey }: { seriesKey: string }) {
  return (
    <Button asChild variant="primary" size="sm" className="w-fit">
      <Link href={practiseHref(seriesKey)}>Practise this</Link>
    </Button>
  );
}

function SuggestionBody({ s, action }: { s: Suggestion; action: string }) {
  return (
    <>
      <Name label={s.label} href={weaknessHref(s.seriesKey, s)} />
      <div className="flex flex-col gap-1">
        <p className="m-0 max-w-prose text-base text-ink [font-variant-numeric:lining-nums]">
          {goalText({
            targetRate: s.target.rate,
            baselineRate: s.baseline.rate,
            gamesN: s.gamesN,
            timeClass: s.timeClass,
          })}
        </p>
        <p className="m-0 max-w-prose text-sm text-ink-soft [font-variant-numeric:lining-nums]">
          {targetWhyText(s)}
          {colourNote(s.side)}
        </p>
      </div>
      <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
        <SuggestFocusControls
          target={{
            accountId: s.accountId,
            timeClass: s.timeClass,
            seriesKey: s.seriesKey,
            side: s.side,
          }}
          label={action}
        />
        <Link href={`/app/weaknesses?${query(s)}`} className={`text-sm ${LINK}`}>
          Choose another
        </Link>
      </div>
    </>
  );
}

export function FocusCard({ card }: { card: FocusCardModel }) {
  switch (card.kind) {
    case "active": {
      const { focus: f, progress: p } = card;
      return (
        <Sheet title="This week's focus">
          <Name label={f.label} href={weaknessHref(f.seriesKey, f)} />
          <p className="m-0 max-w-prose text-base text-ink [font-variant-numeric:lining-nums]">
            {goalText({
              targetRate: f.targetRate,
              baselineRate: f.baseline.rate,
              gamesN: f.gamesN,
              timeClass: f.timeClass,
            })}
            {colourNote(f.side)}
          </p>
          <GamesBar progress={p} />
          <div className="flex flex-col gap-2">
            <Pill
              text={verdictText(p.verdict)}
              provisional={p.verdict.kind === "too_early"}
              data={p.verdict.kind}
            />
            <p className="m-0 max-w-prose text-sm text-ink-soft [font-variant-numeric:lining-nums]">
              {progressText(p)} The target is {pctText(f.targetRate)}.
            </p>
          </div>
          <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
            <Practise seriesKey={f.seriesKey} />
            <ActiveFocusControls timeClass={f.timeClass} gamesN={f.gamesN} />
          </div>
        </Sheet>
      );
    }
    case "closed": {
      const { focus: f, progress: p, next } = card;
      const outcome = p.outcome ?? "inconclusive";
      return (
        <Sheet title="Your last focus">
          <Name label={f.label} href={weaknessHref(f.seriesKey, f)} />
          <div className="flex flex-col gap-2">
            <Pill
              text={outcomeText(outcome)}
              provisional={outcome === "inconclusive"}
              data={outcome}
            />
            <p className="m-0 max-w-prose text-sm text-ink-soft [font-variant-numeric:lining-nums]">
              Over {gamesText(p)}: {progressText(p).replace(/^So far /, "")} The target was{" "}
              {pctText(f.targetRate)}, from {pctText(f.baseline.rate)} before.
            </p>
          </div>
          {next ? (
            <div className="flex flex-col gap-4 pt-2">
              <h3 className="m-0 text-base font-medium text-ink-soft">Next</h3>
              <SuggestionBody s={next} action="Make this my next focus" />
            </div>
          ) : null}
        </Sheet>
      );
    }
    case "suggest":
      return (
        <Sheet title="A focus for this week">
          <SuggestionBody s={card.suggestion} action="Make this my focus" />
        </Sheet>
      );
    case "elsewhere":
      return (
        <Sheet title="This week's focus">
          <p className="m-0 max-w-prose text-base text-ink">
            Your {card.focus.timeClass} focus, {card.focus.label}, is on your other linked account.
            Switch to it to see its progress.
          </p>
        </Sheet>
      );
    case "none":
      return null;
  }
}
