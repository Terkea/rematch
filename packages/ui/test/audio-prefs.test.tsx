// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useBoardSounds } from "../src/audio/index";

beforeEach(() => window.localStorage.clear());

describe("useBoardSounds preferences", () => {
  it("defaults to sound on at 60 percent", () => {
    const { result } = renderHook(() => useBoardSounds());
    expect(result.current.muted).toBe(false);
    expect(result.current.volume).toBeCloseTo(0.6);
  });

  it("remembers mute and volume across mounts", () => {
    const first = renderHook(() => useBoardSounds());
    act(() => first.result.current.setMuted(true));
    act(() => first.result.current.setVolume(0.25));
    first.unmount();
    const second = renderHook(() => useBoardSounds());
    expect(second.result.current.muted).toBe(true);
    expect(second.result.current.volume).toBeCloseTo(0.25);
  });

  it("falls back to the defaults when storage is unreadable", () => {
    window.localStorage.setItem("ci.sound", "{not json");
    const { result } = renderHook(() => useBoardSounds());
    expect(result.current.muted).toBe(false);
  });
});
