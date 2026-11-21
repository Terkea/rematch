// The small path dialect the annotation geometry speaks: absolute M, L, Q, A and Z only, numbers
// in board units to 4 decimals. Enough to hand paths between geometry, roughen and the layer.

export type PathCommand =
  | { type: "M" | "L"; x: number; y: number }
  | { type: "Q"; cx: number; cy: number; x: number; y: number }
  | { type: "A"; r: number; sweep: 0 | 1; x: number; y: number }
  | { type: "Z" };

const n = (v: number) => {
  const s = v.toFixed(4).replace(/\.?0+$/, "");
  return s === "-0" ? "0" : s;
};

export function formatPath(cmds: readonly PathCommand[]): string {
  return cmds
    .map((c) => {
      switch (c.type) {
        case "M":
        case "L":
          return `${c.type}${n(c.x)} ${n(c.y)}`;
        case "Q":
          return `Q${n(c.cx)} ${n(c.cy)} ${n(c.x)} ${n(c.y)}`;
        case "A":
          return `A${n(c.r)} ${n(c.r)} 0 0 ${c.sweep} ${n(c.x)} ${n(c.y)}`;
        default:
          return "Z";
      }
    })
    .join(" ");
}

export function parsePath(d: string): PathCommand[] {
  const tokens = d.match(/[MLQAZ]|-?\d*\.?\d+(?:e-?\d+)?/gi) ?? [];
  const out: PathCommand[] = [];
  let i = 0;
  const num = () => Number(tokens[i++]);
  while (i < tokens.length) {
    const type = (tokens[i++] ?? "").toUpperCase();
    if (type === "M" || type === "L") out.push({ type, x: num(), y: num() });
    else if (type === "Q") out.push({ type, cx: num(), cy: num(), x: num(), y: num() });
    else if (type === "A") {
      const r = num();
      i += 3; // ry, rotation, large-arc
      const sweep = num() ? 1 : 0;
      out.push({ type, r, sweep, x: num(), y: num() });
    } else if (type === "Z") out.push({ type: "Z" });
    else throw new Error(`Unsupported path command "${type}" in "${d}"`);
  }
  return out;
}
