import { defaultContextFactory } from "./context.js";
import type { BoardSound } from "./sounds.js";
import { playInto } from "./synth.js";

export interface SoundEngine {
  /** Create the context and resume it. Resolves true once it is running; false means try again on the next gesture. */
  unlock(): Promise<boolean>;
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

  const make = () => {
    ctx = factory();
    master = ctx.createGain();
    master.gain.value = volume;
    master.connect(ctx.destination);
  };
  const resume = async (c: AudioContext) => {
    try {
      await c.resume();
    } catch {
      // Refused (no user activation yet): the next gesture tries again.
    }
  };

  return {
    async unlock() {
      try {
        if (!ctx || ctx.state === "closed") make();
        const c = ctx;
        if (c && c.state !== "running") await resume(c);
        unlocked = c?.state === "running";
      } catch (e) {
        unlocked = false;
        if (process.env.NODE_ENV !== "production") console.warn("[sound] unlock failed", e);
      }
      return unlocked;
    },
    play(s) {
      try {
        if (!unlocked || muted || !ctx || !master) return;
        if (ctx.state === "closed") {
          // Dropped by the browser: make a fresh one lazily and let it resume; this sound is skipped.
          make();
          if (ctx) void resume(ctx);
          return;
        }
        if (ctx.state !== "running") {
          // Suspended or interrupted (backgrounded tab, iOS call): wake it for the next play.
          void resume(ctx);
          return;
        }
        const t = now();
        if (t - last < THROTTLE_MS) return;
        last = t;
        playInto(ctx, master, s, ctx.currentTime + 0.005);
      } catch (e) {
        // A sound must never break stepping.
        if (process.env.NODE_ENV !== "production") console.warn("[sound] play failed", e);
      }
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
