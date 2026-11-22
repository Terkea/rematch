import type { ReviewGame } from "../../../../../server/games/review";

// The review header's lines, rendered on the server (they never reach the client view).

type HeaderGame = Pick<
  ReviewGame,
  | "white"
  | "black"
  | "playedAt"
  | "timeClass"
  | "baseSeconds"
  | "incrementSeconds"
  | "rated"
  | "playerResult"
  | "playerColor"
  | "accuracy"
>;

const TIME_CLASS: Record<string, string> = {
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  daily: "Daily",
};

const DAY = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
  timeZone: "UTC",
});

/** "12 Nov 2026", in UTC (ruling R6). */
export const dayText = (d: Date) => DAY.format(d);

/** The document title: "<White> v <Black>, 12 Nov 2026". */
export function pageTitle(g: HeaderGame): string {
  return `${g.white.name} v ${g.black.name}, ${dayText(g.playedAt)}`;
}

/** "Rapid 10+0, rated, 12 Nov 2026"; no control when the base isn't whole minutes (daily). */
export function metaLine(g: HeaderGame): string {
  const tc = TIME_CLASS[g.timeClass] ?? g.timeClass;
  const control =
    g.baseSeconds !== null && g.baseSeconds > 0 && g.baseSeconds % 60 === 0
      ? ` ${g.baseSeconds / 60}+${g.incrementSeconds ?? 0}`
      : "";
  return `${tc}${control}, ${g.rated ? "rated" : "casual"}, ${dayText(g.playedAt)}`;
}

export function resultLine(g: HeaderGame): string {
  switch (g.playerResult) {
    case "win":
      return "You won";
    case "loss":
      return "You lost";
    case "draw":
      return "Draw";
    default:
      return "Unfinished";
  }
}

/** ["Your accuracy 71.3%", "Opponent 64.0%"], leaving out what isn't known. */
export function accuracyLine(g: HeaderGame): string[] {
  const mine = g.accuracy[g.playerColor];
  const theirs = g.accuracy[g.playerColor === "white" ? "black" : "white"];
  return [
    mine === null ? null : `Your accuracy ${mine.toFixed(1)}%`,
    theirs === null ? null : `Opponent ${theirs.toFixed(1)}%`,
  ].filter((s): s is string => s !== null);
}
