// Three sound sets for the owner's listening pass: variations of the shipped procedural synth
// (packages/ui/src/audio/synth.ts), all "paper and wood: quiet, close and dry". Nothing is
// sampled. Lab only: the chosen set's numbers go into synth.ts and DESIGN.md "Sound" afterwards.

export type AuditionSound = "move" | "capture" | "check" | "correct" | "wrong" | "gameEnd";

export interface SoundSet {
  id: "a" | "b" | "c";
  name: string;
  description: string;
  tradeOff: string;
  place: {
    noiseHz: number;
    noiseQ: number;
    noiseDecay: number;
    noisePeak: number;
    thumpHz: number;
    thumpTo: number;
    thumpDecay: number;
    thumpPeak: number;
  };
  capture: { second: number; gain: number; pitch: number };
  check: { type: OscillatorType; hz: number; peak: number; decay: number; double: boolean };
  correct: { hz: number[]; gap: number; peak: number; decay: number };
  wrong: {
    hz: number;
    to: number;
    peak: number;
    decay: number;
    noiseHz: number;
    noisePeak: number;
  };
  gameEnd: { hz: number[]; peak: number; decay: number };
  /** The parameters in words, for the page. */
  spec: Record<AuditionSound, string>;
}

export const SOUND_SETS: SoundSet[] = [
  {
    id: "a",
    name: "Club table",
    description:
      "The shipped set. A wooden piece set down on a hard table: a short click over a small low knock. Check adds a light tick; the drill chime is a rising fifth.",
    tradeOff:
      "Already tuned against the motion timings, but the chime is the brightest sound in the app and can read as a game show at full volume.",
    place: {
      noiseHz: 1800,
      noiseQ: 1.1,
      noiseDecay: 0.035,
      noisePeak: 0.55,
      thumpHz: 180,
      thumpTo: 120,
      thumpDecay: 0.06,
      thumpPeak: 0.7,
    },
    capture: { second: 0.028, gain: 0.6, pitch: 1.1 },
    check: { type: "triangle", hz: 1200, peak: 0.16, decay: 0.07, double: false },
    correct: { hz: [523.25, 784], gap: 0.09, peak: 0.2, decay: 0.34 },
    wrong: { hz: 110, to: 80, peak: 0.5, decay: 0.16, noiseHz: 400, noisePeak: 0.2 },
    gameEnd: { hz: [98, 147, 196], peak: 0.2, decay: 0.9 },
    spec: {
      move: "noise 1.8 kHz, Q 1.1, 35 ms; sine 180 to 120 Hz, 60 ms",
      capture: "the move at gain 1.35, pitch 0.92, then a second at 0.6, pitch 1.1, 28 ms later",
      check: "the move, then a triangle at 1.2 kHz, 70 ms, peak 0.16",
      correct: "sines at 523 Hz and 784 Hz, 90 ms apart",
      wrong: "sine 110 to 80 Hz, 160 ms, with noise at 400 Hz",
      gameEnd: "sines at 98, 147 and 196 Hz, 900 ms",
    },
  },
  {
    id: "b",
    name: "Felt base",
    description:
      "Pieces with felt under them, on a softer board. Lower and rounder: more knock, less click. Check is a soft wooden tap; the drill chime drops to a fourth and is quieter.",
    tradeOff:
      "The calmest of the three and the easiest to leave on for an hour, but a capture is harder to tell from a move without looking.",
    place: {
      noiseHz: 1200,
      noiseQ: 0.8,
      noiseDecay: 0.045,
      noisePeak: 0.38,
      thumpHz: 140,
      thumpTo: 95,
      thumpDecay: 0.085,
      thumpPeak: 0.75,
    },
    capture: { second: 0.034, gain: 0.7, pitch: 1.06 },
    check: { type: "sine", hz: 880, peak: 0.14, decay: 0.09, double: false },
    correct: { hz: [440, 587.33], gap: 0.11, peak: 0.15, decay: 0.38 },
    wrong: { hz: 92, to: 66, peak: 0.45, decay: 0.2, noiseHz: 300, noisePeak: 0.14 },
    gameEnd: { hz: [82.4, 123.5, 164.8], peak: 0.18, decay: 1.1 },
    spec: {
      move: "noise 1.2 kHz, Q 0.8, 45 ms; sine 140 to 95 Hz, 85 ms",
      capture: "the move at gain 1.35, pitch 0.92, then a second at 0.7, pitch 1.06, 34 ms later",
      check: "the move, then a sine at 880 Hz, 90 ms, peak 0.14",
      correct: "sines at 440 Hz and 587 Hz, 110 ms apart, peak 0.15",
      wrong: "sine 92 to 66 Hz, 200 ms, with noise at 300 Hz",
      gameEnd: "sines at 82, 124 and 165 Hz, 1.1 s",
    },
  },
  {
    id: "c",
    name: "Pencil and paper",
    description:
      "The driest set: a tight, high click with almost no body, like a pencil tapped on a scoresheet. Check is two quick ticks; the drill chime is one short note.",
    tradeOff:
      "The most precise and the closest to the scoresheet idea, but on laptop speakers the move click is thin and can sound like a UI tick rather than a piece.",
    place: {
      noiseHz: 2600,
      noiseQ: 1.6,
      noiseDecay: 0.022,
      noisePeak: 0.5,
      thumpHz: 230,
      thumpTo: 165,
      thumpDecay: 0.04,
      thumpPeak: 0.45,
    },
    capture: { second: 0.022, gain: 0.55, pitch: 1.14 },
    check: { type: "triangle", hz: 1600, peak: 0.11, decay: 0.035, double: true },
    correct: { hz: [698.46], gap: 0, peak: 0.17, decay: 0.24 },
    wrong: { hz: 130, to: 100, peak: 0.38, decay: 0.1, noiseHz: 320, noisePeak: 0.22 },
    gameEnd: { hz: [110, 164.8], peak: 0.2, decay: 0.7 },
    spec: {
      move: "noise 2.6 kHz, Q 1.6, 22 ms; sine 230 to 165 Hz, 40 ms",
      capture: "the move at gain 1.35, pitch 0.92, then a second at 0.55, pitch 1.14, 22 ms later",
      check: "the move, then two triangles at 1.6 kHz, 35 ms each, 45 ms apart",
      correct: "one sine at 698 Hz, 240 ms",
      wrong: "sine 130 to 100 Hz, 100 ms, with noise at 320 Hz",
      gameEnd: "sines at 110 and 165 Hz, 700 ms",
    },
  },
];

let shared: AudioContext | null = null;
const noiseCache = new WeakMap<AudioContext, AudioBuffer>();

function noise(ctx: AudioContext): AudioBuffer {
  let b = noiseCache.get(ctx);
  if (!b) {
    b = ctx.createBuffer(1, Math.ceil(ctx.sampleRate * 0.25), ctx.sampleRate);
    const d = b.getChannelData(0);
    let s = 12345;
    for (let i = 0; i < d.length; i++) {
      s = (s * 1664525 + 1013904223) >>> 0;
      d[i] = (s / 0xffffffff) * 2 - 1;
    }
    noiseCache.set(ctx, b);
  }
  return b;
}

function env(ctx: AudioContext, out: AudioNode, t: number, peak: number, decay: number) {
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.0001, t);
  g.gain.linearRampToValueAtTime(peak, t + 0.003);
  g.gain.exponentialRampToValueAtTime(0.0001, t + 0.003 + decay);
  g.connect(out);
  return g;
}

function burst(
  ctx: AudioContext,
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
  ctx: AudioContext,
  out: AudioNode,
  t: number,
  type: OscillatorType,
  hz: number,
  peak: number,
  decay: number,
  to?: number,
) {
  const o = ctx.createOscillator();
  o.type = type;
  o.frequency.setValueAtTime(hz, t);
  if (to) o.frequency.exponentialRampToValueAtTime(to, t + decay);
  o.connect(env(ctx, out, t, peak, decay));
  o.start(t);
  o.stop(t + decay + 0.05);
}

function place(
  ctx: AudioContext,
  out: AudioNode,
  set: SoundSet,
  t: number,
  gain: number,
  pitch = 1,
) {
  const p = set.place;
  burst(ctx, out, t, p.noiseHz * pitch, p.noiseQ, p.noisePeak * gain, p.noiseDecay);
  tone(ctx, out, t, "sine", p.thumpHz * pitch, p.thumpPeak * gain, p.thumpDecay, p.thumpTo * pitch);
}

/** Play one sound of a set at the shipped default volume (60%). Call from a click. */
export async function audition(set: SoundSet, sound: AuditionSound, volume = 0.6): Promise<void> {
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) return;
  shared ??= new Ctor({ latencyHint: "interactive" });
  const ctx = shared;
  if (ctx.state !== "running") await ctx.resume();
  const out = ctx.createGain();
  out.gain.value = volume;
  out.connect(ctx.destination);
  const t = ctx.currentTime + 0.01;
  switch (sound) {
    case "move":
      place(ctx, out, set, t, 1);
      break;
    case "capture":
      place(ctx, out, set, t, 1.35, 0.92);
      place(ctx, out, set, t + set.capture.second, set.capture.gain, set.capture.pitch);
      break;
    case "check": {
      place(ctx, out, set, t, 1);
      const c = set.check;
      tone(ctx, out, t + 0.02, c.type, c.hz, c.peak, c.decay);
      if (c.double) tone(ctx, out, t + 0.065, c.type, c.hz, c.peak * 0.8, c.decay);
      break;
    }
    case "correct":
      set.correct.hz.forEach((hz, i) => {
        tone(
          ctx,
          out,
          t + i * set.correct.gap,
          "sine",
          hz,
          set.correct.peak,
          set.correct.decay + i * 0.08,
        );
      });
      break;
    case "wrong": {
      const w = set.wrong;
      tone(ctx, out, t, "sine", w.hz, w.peak, w.decay, w.to);
      burst(ctx, out, t, w.noiseHz, 0.8, w.noisePeak, 0.06);
      break;
    }
    case "gameEnd":
      for (const hz of set.gameEnd.hz)
        tone(ctx, out, t, "sine", hz, set.gameEnd.peak, set.gameEnd.decay);
      break;
  }
}
