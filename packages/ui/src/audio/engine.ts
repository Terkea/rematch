import { defaultContextFactory } from "./context.js";
import type { BoardSound } from "./sounds.js";
import { playInto } from "./synth.js";

export interface SoundEngine {
  unlock(): Promise<void>;
  play(s: BoardSound): void;
  setVolume(v: number): void;
  setMuted(m: boolean): void;
}

const THROTTLE_MS = 40;

export function createSoundEngine(
  opts: { ctxFactory?: () => AudioContext; now?: () => number; volume?: number } = {},
): SoundEngine {
  const factory = opts.ctxFactory ?? defaultContextFactory;
  const now = opts.now ?? (() => performance.now());
  let ctx: AudioContext | null = null;
  let master: GainNode | null = null;
  let unlocked = false;
  let muted = false;
  let volume = opts.volume ?? 0.6;
  let last = Number.NEGATIVE_INFINITY;

  return {
    async unlock() {
      if (!ctx) {
        ctx = factory();
        master = ctx.createGain();
        master.gain.value = volume;
        master.connect(ctx.destination);
      }
      if (ctx.state === "suspended") await ctx.resume();
      unlocked = true;
    },
    play(s) {
      if (!unlocked || muted || !ctx || !master) return;
      const t = now();
      if (t - last < THROTTLE_MS) return;
      last = t;
      playInto(ctx, master, s, ctx.currentTime + 0.005);
    },
    setVolume(v) {
      volume = Math.min(1, Math.max(0, v));
      if (master) master.gain.value = volume;
    },
    setMuted(m) {
      muted = m;
    },
  };
}
