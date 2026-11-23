import { weaknessDef } from "../../../../server/profile/catalogue";
import {
  MIN_CHANCES,
  type Rate,
  type SourceKey,
  type Verdict,
} from "../../../../server/train/model";

// The train page's words (ADR 0090). Every rate carries its n and its stored-style 80% range, a
// missing rate is a dash, and no sentence claims a change unless the ranges separate.

const label = (key: string) => weaknessDef(key)?.label ?? key;

/** One name per source: the catalogue's plain names (ADR 0087), and "Blunders" for the judgement. */
export const SOURCE_LABEL: Record<SourceKey, string> = {
  "W4.missed": label("W4.missed"),
  "W3.shallow": label("W3.shallow"),
  "W3.deep": label("W3.deep"),
  "W7.traps": label("W7.traps"),
  blunder: "Blunders",
};

const count = new Intl.NumberFormat("en-GB");
const chances = (n: number) => `${count.format(n)} ${n === 1 ? "chance" : "chances"}`;
const pct1 = (v: number) => `${(v * 100).toFixed(1)}%`;

/** "14.6% of 91 chances (80% range 11–19%)"; a dash when there are no chances. */
export function rateText(r: Rate): string {
  if (r.chances <= 0 || r.rate === null) return "– (no chances yet)";
  const range =
    r.low !== null && r.high !== null
      ? ` (80% range ${Math.floor(r.low * 100)}–${Math.ceil(r.high * 100)}%)`
      : "";
  return `${pct1(r.rate)} of ${chances(r.chances)}${range}`;
}

const VERDICT: Record<Exclude<Verdict, "too_early">, string> = {
  lower: "Lower since your first drill",
  higher: "Higher since your first drill",
  no_clear_change: "No clear change yet: the ranges overlap.",
};

/** One source's line in "Is it working?". */
export function sourceRowText(r: {
  firstDrill: Date | null;
  before: Rate;
  after: Rate;
  verdict: Verdict;
}): string {
  if (r.firstDrill === null) return "Not drilled yet.";
  if (r.verdict === "too_early") {
    if (r.after.chances < MIN_CHANCES)
      return `Too early to tell: ${chances(r.after.chances)} in games since your first drill, and ${MIN_CHANCES} are needed.`;
    return `Too early to tell: ${chances(r.before.chances)} in games before your first drill, and ${MIN_CHANCES} are needed.`;
  }
  const sides = `${rateText(r.after)} since; ${rateText(r.before)} before.`;
  if (r.verdict === "no_clear_change") return `${VERDICT.no_clear_change} ${sides}`;
  return `${VERDICT[r.verdict]}: ${sides}`;
}

/** The drilled-versus-held-back line: how often each pattern was found in later games. */
export function patternText(p: {
  since: Date | null;
  drilled: Rate;
  heldOut: Rate;
  verdict: Verdict;
}): string {
  if (p.since === null) return "Not drilled yet.";
  if (p.drilled.chances < MIN_CHANCES || p.heldOut.chances < MIN_CHANCES)
    return `Too early to tell: ${chances(p.drilled.chances)} at the patterns you drilled and ${count.format(p.heldOut.chances)} at those held back, and each needs ${MIN_CHANCES}.`;
  const both = `Found in later games: patterns you drilled ${rateText(p.drilled)}; patterns held back ${rateText(p.heldOut)}.`;
  if (p.verdict === "higher") return `${both} The drilled ones are found more often.`;
  if (p.verdict === "lower") return `${both} The drilled ones are found less often.`;
  return `${both} No clear difference yet: the ranges overlap.`;
}
