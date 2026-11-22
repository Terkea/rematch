import type { BoardSound } from "./sounds.js";

// Original, procedural sounds: paper and wood, quiet, close and dry (DESIGN.md "Sound"). Nothing
// is sampled and nothing imitates another product. Every sound routes through one bus gain, so
// the master volume scales them all and an engine can count one voice per play.

/** The placement voice: a band-passed noise tick over a short low sine body. */
export const PLACEMENT = {
  noiseHz: 1800,
  noiseQ: 1.1,
  noiseDecay: 0.035,
  thumpHz: 180,
  thumpDecay: 0.06,
} as const;

type Ctx = AudioContext;

const noiseCache = new WeakMap<Ctx, AudioBuffer>();

function noise(ctx: Ctx): AudioBuffer {
  let b = noiseCache.get(ctx);
  if (!b) {
    b = ctx.createBuffer(1, Math.ceil(ctx.sampleRate * 0.25), ctx.sampleRate);
    const d = b.getChannelData(0);
    // A fixed linear congruential sequence, so the sound is identical every time.
    let s = 12345;
    for (let i = 0; i < d.length; i++) {
      s = (s * 1664525 + 1013904223) >>> 0;
      d[i] = (s / 0xffffffff) * 2 - 1;
    }
    noiseCache.set(ctx, b);
  }
  return b;
}

function env(ctx: Ctx, out: AudioNode, t: number, peak: number, decay: number): GainNode {
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.0001, t);
  g.gain.linearRampToValueAtTime(peak, t + 0.003);
  g.gain.exponentialRampToValueAtTime(0.0001, t + 0.003 + decay);
  g.connect(out);
  return g;
}

function burst(
  ctx: Ctx,
  out: AudioNode,
  t: number,
  hz: number,
  q: number,
  peak: number,
  decay: number,
) {
  const src = ctx.createBufferSource();
  src.buffer = noise(ctx);
  const f = ctx.createBiquadFilter();
  f.type = "bandpass";
  f.frequency.value = hz;
  f.Q.value = q;
  src.connect(f);
  f.connect(env(ctx, out, t, peak, decay));
  src.start(t);
  src.stop(t + decay + 0.05);
}

function tone(
  ctx: Ctx,
  out: AudioNode,
  t: number,
  type: OscillatorType,
  hz: number,
  peak: number,
  decay: number,
  glideTo?: number,
) {
  const o = ctx.createOscillator();
  o.type = type;
  o.frequency.setValueAtTime(hz, t);
  if (glideTo) o.frequency.exponentialRampToValueAtTime(glideTo, t + decay);
  o.connect(env(ctx, out, t, peak, decay));
  o.start(t);
  o.stop(t + decay + 0.05);
}

function placement(ctx: Ctx, out: AudioNode, t: number, gain: number, pitch = 1) {
  const p = PLACEMENT;
  burst(ctx, out, t, p.noiseHz * pitch, p.noiseQ, 0.55 * gain, p.noiseDecay);
  tone(ctx, out, t, "sine", p.thumpHz * pitch, 0.7 * gain, p.thumpDecay, 120 * pitch);
}

/** Schedule one sound's voices into `out`, starting at audio time `t`. */
export function synthesize(ctx: Ctx, out: AudioNode, sound: BoardSound, t: number): void {
  switch (sound) {
    case "move":
      placement(ctx, out, t, 1);
      break;
    case "capture":
      placement(ctx, out, t, 1.35, 0.92);
      placement(ctx, out, t + 0.028, 0.6, 1.1);
      break;
    case "check":
      placement(ctx, out, t, 1);
      tone(ctx, out, t + 0.02, "triangle", 1200, 0.16, 0.07);
      break;
    case "castle":
      placement(ctx, out, t, 1);
      placement(ctx, out, t + 0.07, 0.9, 1.08);
      break;
    case "promote":
      placement(ctx, out, t, 1);
      tone(ctx, out, t + 0.06, "sine", 660, 0.18, 0.12);
      tone(ctx, out, t + 0.12, "sine", 990, 0.16, 0.2);
      break;
    case "gameEnd":
      for (const hz of [98, 147, 196]) tone(ctx, out, t, "sine", hz, 0.2, 0.9);
      break;
    case "correct":
      tone(ctx, out, t, "sine", 523.25, 0.2, 0.28);
      tone(ctx, out, t + 0.09, "sine", 784, 0.18, 0.4);
      break;
    case "wrong":
      tone(ctx, out, t, "sine", 110, 0.5, 0.16, 80);
      burst(ctx, out, t, 400, 0.8, 0.2, 0.06);
      break;
    case "pen":
      burst(ctx, out, t, 3200, 0.7, 0.05, 0.14);
      break;
  }
}

/** One voice bus per play: exactly one node connects into `master`. */
export function playInto(ctx: Ctx, master: AudioNode, sound: BoardSound, t: number): void {
  const bus = ctx.createGain();
  bus.connect(master);
  synthesize(ctx, bus, sound, t);
}
