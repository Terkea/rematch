import type { TimeClass } from "../server/games/types";

/**
 * The order every time-control list, chip row and scope switch uses (I8, the owner's ruling):
 * rapid first, then blitz, then the rest from fastest to slowest. Plain data, no server imports,
 * so client components may use it too.
 */
export const TIME_CLASS_ORDER: readonly TimeClass[] = [
  "rapid",
  "blitz",
  "bullet",
  "classical",
  "daily",
];

/** Compare two time classes by `TIME_CLASS_ORDER`. */
export const byTimeClass = (a: TimeClass, b: TimeClass) =>
  TIME_CLASS_ORDER.indexOf(a) - TIME_CLASS_ORDER.indexOf(b);

/** The class a time-control switch selects when the URL names none: rapid when there is rapid. */
export function defaultTimeClass(available: readonly TimeClass[]): TimeClass | null {
  return TIME_CLASS_ORDER.find((tc) => available.includes(tc)) ?? null;
}

/** Each class's name, sentence case. */
export const TIME_CLASS_LABEL: Readonly<Record<TimeClass, string>> = {
  rapid: "Rapid",
  blitz: "Blitz",
  bullet: "Bullet",
  classical: "Classical",
  daily: "Daily",
};
