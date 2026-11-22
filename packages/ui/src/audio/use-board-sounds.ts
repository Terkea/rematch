"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createSoundEngine, type SoundEngine } from "./engine.js";
import type { BoardSound } from "./sounds.js";

const KEY = "ci.sound";
const DEFAULTS = { muted: false, volume: 0.6, pen: false };
type Prefs = typeof DEFAULTS;

function load(): Prefs {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<Prefs>;
      return {
        muted: p.muted === true,
        volume: typeof p.volume === "number" ? Math.min(1, Math.max(0, p.volume)) : DEFAULTS.volume,
        pen: p.pen === true,
      };
    }
  } catch {
    // storage blocked or corrupt: use the defaults
  }
  return DEFAULTS;
}

let shared: SoundEngine | null = null;
const engine = () => {
  shared ??= createSoundEngine();
  return shared;
};

const GESTURES = ["pointerdown", "keydown"] as const;

/**
 * One engine for the whole app. The audio context unlocks on the first pointer or key gesture,
 * never before (autoplay rules). Reduced motion does not mute sound; sound has its own setting.
 */
export function useBoardSounds() {
  const [prefs, setPrefs] = useState<Prefs>(DEFAULTS);
  const prefsRef = useRef(prefs);
  prefsRef.current = prefs;

  useEffect(() => {
    const p = load();
    setPrefs(p);
    prefsRef.current = p;
    const e = engine();
    e.setMuted(p.muted);
    e.setVolume(p.volume);
    const off = () => {
      for (const ev of GESTURES) window.removeEventListener(ev, unlock, true);
    };
    const unlock = () => {
      e.unlock().catch(() => undefined);
      off();
    };
    for (const ev of GESTURES) window.addEventListener(ev, unlock, true);
    return off;
  }, []);

  const update = useCallback((patch: Partial<Prefs>) => {
    const next = { ...prefsRef.current, ...patch };
    prefsRef.current = next;
    setPrefs(next);
    try {
      window.localStorage.setItem(KEY, JSON.stringify(next));
    } catch {
      // not persisted; the choice still applies on this page
    }
  }, []);

  const setMuted = useCallback(
    (m: boolean) => {
      engine().setMuted(m);
      update({ muted: m });
    },
    [update],
  );
  const setVolume = useCallback(
    (v: number) => {
      engine().setVolume(v);
      update({ volume: v });
    },
    [update],
  );
  const play = useCallback((s: BoardSound) => {
    if (s === "pen" && !prefsRef.current.pen) return; // ambient, off by default
    engine().play(s);
  }, []);

  return useMemo(
    () => ({ play, muted: prefs.muted, setMuted, volume: prefs.volume, setVolume }),
    [play, prefs.muted, prefs.volume, setMuted, setVolume],
  );
}
