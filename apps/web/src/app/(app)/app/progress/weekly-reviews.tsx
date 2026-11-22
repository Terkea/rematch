import { AiDisclosure, CoachState, SegmentText } from "@ci/ui/coach";
import { dayText } from "@ci/ui/profile";
import { ProgressPlaceholder } from "@ci/ui/progress";
import { segmentHref } from "../../../../lib/segment-href";
import type { NarrativeItem, WeeklyReviewView } from "../../../../server/coaching";

// Weekly reviews (spec G2: Rust writes them each Monday for the week before). Model text is shown
// only for a `ready` review with its validated content and its stored disclosure (ADR 0060,
// spec §14); everything else gets an honest state, and nothing is shown as written that is not.

export const WEEKLY_EMPTY = "Weekly reviews appear here once your coach writes them.";
export const WEEKLY_PENDING = "Your weekly review is being written";
export const WEEKLY_UNCHECKED =
  "This week's review couldn't be checked against your games, so it isn't shown.";

/** The first review: the earliest week with games whose review day (the Monday after) is today or later. */
export type FirstReview = { week: string; due: string } | null;

function Week({ start }: { start: string }) {
  return (
    <h3 className="m-0 font-serif text-xl font-semibold text-ink">
      Week of{" "}
      <time dateTime={start} className="[font-variant-numeric:lining-nums]">
        {dayText(start)}
      </time>
    </h3>
  );
}

function Text({ item, className }: { item: NarrativeItem; className?: string }) {
  return <SegmentText segments={item.segments} hrefFor={segmentHref} className={className} />;
}

function Part({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h4 className="m-0 text-sm font-medium text-ink-soft">{title}</h4>
      {children}
    </div>
  );
}

function Ready({ review }: { review: WeeklyReviewView }) {
  const c = review.content;
  const d = review.aiDisclosure;
  if (!c || !d) return <CoachState kind="unavailable" title={WEEKLY_UNCHECKED} titleAs="p" />;
  return (
    <div className="flex flex-col gap-5">
      <AiDisclosure disclosure={d} compact />
      {c.headline ? (
        <Text item={c.headline} className="m-0 max-w-2xl font-serif text-xl text-ink" />
      ) : null}
      <div className="grid grid-cols-1 gap-x-10 gap-y-5 lg:grid-cols-2">
        {c.process ? (
          <Part title="How you played">
            <Text item={c.process} className="m-0 text-base text-ink" />
          </Part>
        ) : null}
        {c.outcomes.length > 0 ? (
          <Part title="Outcomes">
            <ul className="m-0 flex list-none flex-col gap-2 p-0">
              {c.outcomes.map((o) => (
                <li key={`${o.weaknessId}-${o.label}`} className="flex flex-col gap-0.5">
                  <span className="text-base font-medium text-ink">{o.label}</span>
                  <Text item={o} className="m-0 text-base text-ink" />
                </li>
              ))}
            </ul>
          </Part>
        ) : null}
        {c.exemplars.length > 0 ? (
          <Part title="Positions from the week">
            <ul className="m-0 flex list-none flex-col gap-2 p-0">
              {c.exemplars.map((e, i) => (
                // Exemplars carry no id of their own; their order is the stored order.
                // biome-ignore lint/suspicious/noArrayIndexKey: stored order, never reordered
                <li key={i}>
                  <Text item={e} className="m-0 text-base text-ink" />
                </li>
              ))}
            </ul>
          </Part>
        ) : null}
        {c.nextWeek ? (
          <Part title="Next week">
            <Text item={c.nextWeek} className="m-0 text-base text-ink" />
          </Part>
        ) : null}
      </div>
      {review.droppedCount > 0 ? (
        <p className="m-0 text-sm text-ink-faint">
          {review.droppedCount === 1
            ? "One part couldn't be checked against your games, so it isn't shown."
            : `${review.droppedCount} parts couldn't be checked against your games, so they aren't shown.`}
        </p>
      ) : null}
    </div>
  );
}

function Review({ review }: { review: WeeklyReviewView }) {
  return (
    <article className="flex flex-col gap-4 border-b border-rule pb-8 last:border-b-0 last:pb-0">
      <Week start={review.weekStart} />
      {review.status === "ready" ? (
        <Ready review={review} />
      ) : review.status === "pending" ? (
        <div className="flex flex-col gap-3">
          <p className="m-0 text-base text-ink-soft">{WEEKLY_PENDING}</p>
          <ProgressPlaceholder shape="list" verb="Writing" />
        </div>
      ) : (
        <CoachState kind="unavailable" title={WEEKLY_UNCHECKED} titleAs="p" />
      )}
    </article>
  );
}

export function WeeklyReviews({
  reviews,
  first,
}: {
  reviews: WeeklyReviewView[];
  first: FirstReview;
}) {
  if (reviews.length === 0) {
    return (
      <div className="flex flex-col gap-3">
        <CoachState kind="empty" title={WEEKLY_EMPTY} titleAs="p" />
        <p className="m-0 max-w-2xl text-base text-ink-soft [font-variant-numeric:lining-nums]">
          {first ? (
            <>
              Your first, for the week of <time dateTime={first.week}>{dayText(first.week)}</time>,
              is scheduled for <time dateTime={first.due}>{dayText(first.due)}</time>.
            </>
          ) : (
            "Each Monday your coach reviews the week before, for any week you played in."
          )}
        </p>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-8">
      {reviews.map((r) => (
        <Review key={r.id} review={r} />
      ))}
    </div>
  );
}
