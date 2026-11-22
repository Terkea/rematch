// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The hook keeps one engine per module, so each test loads a fresh copy.
function stubAudio(refuse: { on: boolean }) {
  let resumes = 0;
  class Ctx {
    state = "suspended";
    currentTime = 0;
    sampleRate = 8000;
    destination = {};
    async resume() {
      resumes++;
      if (refuse.on) throw new Error("NotAllowedError");
      this.state = "running";
    }
    createGain() {
      return { gain: { value: 0 }, connect() {} };
    }
  }
  vi.stubGlobal("AudioContext", Ctx);
  Object.defineProperty(window, "AudioContext", { configurable: true, value: Ctx });
  return () => resumes;
}

beforeEach(() => {
  window.localStorage.clear();
  vi.resetModules();
});
afterEach(() => vi.unstubAllGlobals());

describe("useBoardSounds unlock", () => {
  it("keeps listening on every gesture kind until the context runs", async () => {
    const refuse = { on: true };
    const resumes = stubAudio(refuse);
    const { useBoardSounds } = await import("../src/audio/index");
    renderHook(() => useBoardSounds());

    await act(async () => {
      window.dispatchEvent(new Event("pointerdown"));
    });
    expect(resumes()).toBe(1);
    // refused: the listeners must still be there, for touchend and click too
    await act(async () => {
      window.dispatchEvent(new Event("touchend"));
    });
    expect(resumes()).toBe(2);
    refuse.on = false;
    await act(async () => {
      window.dispatchEvent(new Event("click"));
    });
    expect(resumes()).toBe(3);
    // running now: the listeners are gone
    await act(async () => {
      window.dispatchEvent(new Event("keydown"));
    });
    expect(resumes()).toBe(3);
  });
});
