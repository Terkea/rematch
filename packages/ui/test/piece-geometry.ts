// Flattens a generated piece (SVG paths, circles, transforms) into points in its 45 × 45 drawing,
// each with the stroke's half width around it, so tests can check exactly where a piece paints.
import type { PieceNode } from "../scripts/build-pieces-lib.js";

export interface InkPoint {
  x: number;
  y: number;
  /** half the stroke width (0 for a fill-only point), in drawing units */
  r: number;
}

type Matrix = [number, number, number, number, number, number];
const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];

function multiply(m: Matrix, n: Matrix): Matrix {
  return [
    m[0] * n[0] + m[2] * n[1],
    m[1] * n[0] + m[3] * n[1],
    m[0] * n[2] + m[2] * n[3],
    m[1] * n[2] + m[3] * n[3],
    m[0] * n[4] + m[2] * n[5] + m[4],
    m[1] * n[4] + m[3] * n[5] + m[5],
  ];
}

function parseTransform(value: string | undefined): Matrix {
  let m: Matrix = IDENTITY;
  if (!value) return m;
  for (const [, name, args] of value.matchAll(/(\w+)\s*\(([^)]*)\)/g)) {
    const a = (args ?? "")
      .split(/[\s,]+/)
      .filter(Boolean)
      .map(Number);
    let t: Matrix = IDENTITY;
    if (name === "translate") t = [1, 0, 0, 1, a[0] ?? 0, a[1] ?? 0];
    else if (name === "scale") t = [a[0] ?? 1, 0, 0, a[1] ?? a[0] ?? 1, 0, 0];
    else if (name === "matrix") t = a.slice(0, 6) as Matrix;
    else if (name === "rotate") {
      const r = ((a[0] ?? 0) * Math.PI) / 180;
      const [cx, cy] = [a[1] ?? 0, a[2] ?? 0];
      t = multiply(
        multiply([1, 0, 0, 1, cx, cy], [Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]),
        [1, 0, 0, 1, -cx, -cy],
      );
    } else throw new Error(`Unsupported transform ${name}`);
    m = multiply(m, t);
  }
  return m;
}

const apply = (m: Matrix, x: number, y: number): [number, number] => [
  m[0] * x + m[2] * y + m[4],
  m[1] * x + m[3] * y + m[5],
];

/** Points along an SVG path's outline (every command, curves sampled 24 times). */
export function flattenPath(d: string): [number, number][] {
  const tokens = d.match(/[a-zA-Z]|-?(?:\d+\.?\d*|\.\d+)(?:e-?\d+)?/g) ?? [];
  const pts: [number, number][] = [];
  let i = 0;
  let cmd = "";
  let x = 0;
  let y = 0;
  let sx = 0;
  let sy = 0;
  let lastCtrl: [number, number] | null = null;
  const num = () => Number(tokens[i++]);
  const take = (n: number): number[] => Array.from({ length: n }, num);
  const cubic = (x1: number, y1: number, x2: number, y2: number, ex: number, ey: number) => {
    for (let k = 1; k <= 24; k++) {
      const t = k / 24;
      const u = 1 - t;
      pts.push([
        u * u * u * x + 3 * u * u * t * x1 + 3 * u * t * t * x2 + t * t * t * ex,
        u * u * u * y + 3 * u * u * t * y1 + 3 * u * t * t * y2 + t * t * t * ey,
      ]);
    }
    lastCtrl = [x2, y2];
    x = ex;
    y = ey;
  };
  const arc = (
    rx: number,
    ry: number,
    phi: number,
    large: number,
    sweep: number,
    ex: number,
    ey: number,
  ) => {
    // Endpoint to centre parameterisation (SVG 1.1, F.6.5).
    const p = (phi * Math.PI) / 180;
    const cos = Math.cos(p);
    const sin = Math.sin(p);
    const dx = (x - ex) / 2;
    const dy = (y - ey) / 2;
    const x1 = cos * dx + sin * dy;
    const y1 = -sin * dx + cos * dy;
    let a = Math.abs(rx);
    let b = Math.abs(ry);
    const lambda = (x1 * x1) / (a * a) + (y1 * y1) / (b * b);
    if (lambda > 1) {
      a *= Math.sqrt(lambda);
      b *= Math.sqrt(lambda);
    }
    const sign = large === sweep ? -1 : 1;
    const num2 = a * a * b * b - a * a * y1 * y1 - b * b * x1 * x1;
    const co = sign * Math.sqrt(Math.max(0, num2 / (a * a * y1 * y1 + b * b * x1 * x1)));
    const cx1 = (co * a * y1) / b;
    const cy1 = (-co * b * x1) / a;
    const cx = cos * cx1 - sin * cy1 + (x + ex) / 2;
    const cy = sin * cx1 + cos * cy1 + (y + ey) / 2;
    const angle = (ux: number, uy: number, vx: number, vy: number) =>
      Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
    const t1 = angle(1, 0, (x1 - cx1) / a, (y1 - cy1) / b);
    let dt = angle((x1 - cx1) / a, (y1 - cy1) / b, (-x1 - cx1) / a, (-y1 - cy1) / b);
    if (!sweep && dt > 0) dt -= 2 * Math.PI;
    if (sweep && dt < 0) dt += 2 * Math.PI;
    for (let k = 1; k <= 24; k++) {
      const t = t1 + (dt * k) / 24;
      pts.push([
        cx + a * Math.cos(t) * cos - b * Math.sin(t) * sin,
        cy + a * Math.cos(t) * sin + b * Math.sin(t) * cos,
      ]);
    }
    x = ex;
    y = ey;
    lastCtrl = null;
  };
  while (i < tokens.length) {
    if (/[a-zA-Z]/.test(tokens[i] ?? "")) cmd = tokens[i++] as string;
    const rel = cmd === cmd.toLowerCase();
    const ox = rel ? x : 0;
    const oy = rel ? y : 0;
    switch (cmd.toLowerCase()) {
      case "m":
        x = ox + num();
        y = oy + num();
        sx = x;
        sy = y;
        pts.push([x, y]);
        cmd = rel ? "l" : "L";
        lastCtrl = null;
        break;
      case "l":
        x = ox + num();
        y = oy + num();
        pts.push([x, y]);
        lastCtrl = null;
        break;
      case "h":
        x = ox + num();
        pts.push([x, y]);
        lastCtrl = null;
        break;
      case "v":
        y = oy + num();
        pts.push([x, y]);
        lastCtrl = null;
        break;
      case "c": {
        const [x1 = 0, y1 = 0, x2 = 0, y2 = 0, ex = 0, ey = 0] = take(6);
        cubic(ox + x1, oy + y1, ox + x2, oy + y2, ox + ex, oy + ey);
        break;
      }
      case "s": {
        const [x2 = 0, y2 = 0, ex = 0, ey = 0] = take(4);
        const [rx1, ry1] = lastCtrl ? [2 * x - lastCtrl[0], 2 * y - lastCtrl[1]] : [x, y];
        cubic(rx1, ry1, ox + x2, oy + y2, ox + ex, oy + ey);
        break;
      }
      case "a": {
        const [rx = 0, ry = 0, rot = 0, large = 0, sweep = 0, ex = 0, ey = 0] = take(7);
        arc(rx, ry, rot, large, sweep, ox + ex, oy + ey);
        break;
      }
      case "z":
        x = sx;
        y = sy;
        pts.push([x, y]);
        lastCtrl = null;
        break;
      default:
        throw new Error(`Unsupported path command ${cmd}`);
    }
  }
  return pts;
}

/** Every point a piece paints, in its 45 × 45 drawing, with the stroke half width it carries. */
export function inkPoints(nodes: PieceNode[]): InkPoint[] {
  const out: InkPoint[] = [];
  const walk = (list: PieceNode[], m: Matrix, stroke: string, width: number) => {
    for (const node of list) {
      const mm = multiply(m, parseTransform(node.attrs.transform));
      const s = node.style.stroke ?? stroke;
      const w = node.style.strokeWidth !== undefined ? Number(node.style.strokeWidth) : width;
      const scale = Math.sqrt(Math.abs(mm[0] * mm[3] - mm[1] * mm[2]));
      const r = s === "none" ? 0 : (w / 2) * scale;
      let local: [number, number][] = [];
      if (node.tag === "path" && node.attrs.d) local = flattenPath(node.attrs.d);
      if (node.tag === "circle") {
        const [cx, cy, rr] = [Number(node.attrs.cx), Number(node.attrs.cy), Number(node.attrs.r)];
        for (let k = 0; k < 48; k++) {
          const t = (k / 48) * 2 * Math.PI;
          local.push([cx + rr * Math.cos(t), cy + rr * Math.sin(t)]);
        }
      }
      for (const [px, py] of local) {
        const [tx, ty] = apply(mm, px, py);
        out.push({ x: tx, y: ty, r });
      }
      walk(node.children, mm, s, w);
    }
  };
  walk(nodes, IDENTITY, "none", 1);
  return out;
}
