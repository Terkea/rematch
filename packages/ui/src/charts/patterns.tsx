// The overview charts' fills that carry meaning by pattern as well as colour (ADR 0050), so no
// series rests on colour alone: a hatch reads in greyscale, in print and under forced colours.

export type ChartPatternId = "hatch-pen" | "hatch-ink";

/** The `fill` value for a pattern rendered by `<ChartPatterns prefix={prefix} />`. */
export const patternFill = (prefix: string, id: ChartPatternId) => `url(#${prefix}${id})`;

/**
 * Diagonal hatching at 45°, one per token: `hatch-pen` (the player's mistakes) and `hatch-ink`
 * (draws). Each chart renders its own copy with its own `prefix` (from `useId`), so a chart that
 * is hidden or unmounted never takes another chart's fill with it.
 */
export function ChartPatterns({ prefix = "" }: { prefix?: string }) {
  return (
    <svg aria-hidden="true" width={0} height={0} className="absolute" focusable="false">
      <defs>
        <pattern
          id={`${prefix}hatch-pen`}
          width={4}
          height={4}
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect x={0} y={0} width={1.5} height={4} className="fill-pen" />
        </pattern>
        <pattern
          id={`${prefix}hatch-ink`}
          width={5}
          height={5}
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect x={0} y={0} width={1.75} height={5} className="fill-ink" />
        </pattern>
      </defs>
    </svg>
  );
}
