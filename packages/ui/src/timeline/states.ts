import type { WeaknessState } from "../fixtures/types.js";

export type { WeaknessState } from "../fixtures/types.js";

export type StatePattern = "none" | "hatch" | "dots";

export interface StateStyle {
  label: string;
  /** The mark's colour token, as a CSS value. */
  fill: string;
  pattern: StatePattern;
  /**
   * The bar's height as a share of the lane's bar height. A shape cue on top of colour and
   * pattern: a problem fills the lane, `resolved` settles to a band, `not_an_issue` is a hairline.
   */
  weight: "full" | "band" | "hairline";
  /** Hatch angle in degrees (hatch only): improving leans forward, relapsed is cross-hatched. */
  angle?: 45 | "cross";
}

/**
 * The six states of spec §12b. Red keeps its one meaning (the player's weakness is a problem now:
 * emerging, active, relapsed) and green its other (getting better: improving, resolved). No state
 * relies on colour alone (DESIGN.md "Colour never carries meaning alone"): within each hue every
 * state has its own pattern, and the shape of the bar separates the solid ones.
 *
 * - not_an_issue: a `rule-strong` hairline, no bar.
 * - emerging: `pen` dots on the lane's well, the red just starting to show.
 * - active: solid `pen`, full height.
 * - improving: `good` hatch at 45°, full height.
 * - resolved: solid `good`, a thinner band.
 * - relapsed: `pen` cross-hatch, full height.
 */
export const stateStyle: Record<WeaknessState, StateStyle> = {
  not_an_issue: {
    label: "Not an issue",
    fill: "var(--color-rule-strong)",
    pattern: "none",
    weight: "hairline",
  },
  emerging: { label: "Emerging", fill: "var(--color-pen)", pattern: "dots", weight: "full" },
  active: { label: "Active", fill: "var(--color-pen)", pattern: "none", weight: "full" },
  improving: {
    label: "Improving",
    fill: "var(--color-good)",
    pattern: "hatch",
    weight: "full",
    angle: 45,
  },
  resolved: { label: "Resolved", fill: "var(--color-good)", pattern: "none", weight: "band" },
  relapsed: {
    label: "Relapsed",
    fill: "var(--color-pen)",
    pattern: "hatch",
    weight: "full",
    angle: "cross",
  },
};

export const weaknessStates = Object.keys(stateStyle) as WeaknessState[];
