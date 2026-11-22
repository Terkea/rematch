import { describe, expect, it } from "vitest";
import { createSoundEngine, soundForMove } from "../src/audio/index";

describe("soundForMove", () => {
  it.each([
    [{ san: "Nf3", flags: "n" }, "move"],
    [{ san: "Bxe5", flags: "c" }, "capture"],
    [{ san: "O-O", flags: "k" }, "castle"],
    [{ san: "e8=Q", flags: "np" }, "promote"],
    [{ san: "Qxf7+", flags: "c" }, "check"],
    [{ san: "Qxf7#", flags: "c" }, "gameEnd"],
    [{ san: "exd8=Q+", flags: "cp" }, "check"],
  ] as const)("%j -> %s", (m, s) => {
    expect(soundForMove(m)).toBe(s);
  });
});

function fakeCtx() {
  const log = { master: 0, gain: 0.6 };
  const param = () => ({
    value: 0,
    setValueAtTime: () => undefined,
    linearRampToValueAtTime: () => undefined,
    exponentialRampToValueAtTime: () => undefined,
  });
  const node = (): Record<string, unknown> => ({
    connect: () => undefined,
    start: () => undefined,
    stop: () => undefined,
    frequency: param(),
    Q: param(),
    gain: param(),
    type: "",
  });
  let first = true;
  const ctx = {
    currentTime: 0,
    state: "suspended",
    sampleRate: 8000,
    destination: {},
    resume: async () => {
      ctx.state = "running";
    },
    createBuffer: (_c: number, n: number) => ({ getChannelData: () => new Float32Array(n) }),
    createBufferSource: node,
    createOscillator: node,
    createBiquadFilter: node,
    createGain: () => {
      const n = node();
      if (first) {
        // the first gain is the master; the test counts what connects into it
        first = false;
        n.gain = {
          get value() {
            return log.gain;
          },
          set value(v: number) {
            log.gain = v;
          },
        };
        n.isMaster = true;
        return n;
      }
      n.connect = (dest: unknown) => {
        if ((dest as { isMaster?: boolean }).isMaster) log.master++;
      };
      return n;
    },
  };
  return { ctx: ctx as unknown as AudioContext, log };
}

describe("createSoundEngine", () => {
  const make = () => {
    const f = fakeCtx();
    let t = 1000;
    const e = createSoundEngine({ ctxFactory: () => f.ctx, now: () => t });
    return { ...f, e, advance: (ms: number) => (t += ms) };
  };

  it("plays nothing before unlock", () => {
    const { e, log } = make();
    e.play("move");
    expect(log.master).toBe(0);
  });

  it("schedules one voice after unlock and throttles within 40 ms", async () => {
    const { e, log, advance } = make();
    await e.unlock();
    e.play("move");
    expect(log.master).toBe(1);
    advance(20);
    e.play("capture");
    expect(log.master).toBe(1);
    advance(30);
    e.play("capture");
    expect(log.master).toBe(2);
  });

  it("is silent while muted, and sets the master gain", async () => {
    const { e, log } = make();
    await e.unlock();
    e.setMuted(true);
    e.play("move");
    expect(log.master).toBe(0);
    e.setMuted(false);
    e.setVolume(0.3);
    expect(log.gain).toBeCloseTo(0.3);
  });
});
