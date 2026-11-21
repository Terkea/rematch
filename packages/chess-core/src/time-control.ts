export type TimeClass = "bullet" | "blitz" | "rapid" | "classical" | "daily";

/** Classify by estimated duration: base + 40 * increment seconds. */
export function classifyTimeControl(
  baseSeconds: number,
  incrementSeconds: number,
): Exclude<TimeClass, "daily"> {
  const estimated = baseSeconds + 40 * incrementSeconds;
  if (estimated < 180) return "bullet";
  if (estimated < 480) return "blitz";
  if (estimated < 1500) return "rapid";
  return "classical";
}
