"use client";

import { type BoardSound, useBoardSounds } from "@ci/ui/audio";

const SOUNDS: { id: BoardSound; label: string; note: string }[] = [
  { id: "move", label: "Move", note: "a soft wooden placement" },
  { id: "capture", label: "Capture", note: "heavier, with a second knock" },
  { id: "check", label: "Check", note: "a placement with a brighter tick" },
  { id: "castle", label: "Castle", note: "two placements, 70 ms apart" },
  { id: "promote", label: "Promotion", note: "a placement, then a rising pair" },
  { id: "gameEnd", label: "Game end", note: "a low, settled close" },
  { id: "correct", label: "Drill correct", note: "a warm two-note chime" },
  { id: "wrong", label: "Drill wrong", note: "a muted low thud" },
];

export function SoundSection() {
  const { play, muted, setMuted, volume, setVolume } = useBoardSounds();
  return (
    <section
      id="sound"
      data-section="sound"
      aria-labelledby="sound-title"
      className="flex flex-col gap-6 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex max-w-prose flex-col gap-1">
        <h2 id="sound-title" className="font-serif text-2xl font-semibold">
          Sound
        </h2>
        <p className="text-sm text-ink-soft">
          Paper and wood: quiet, close and dry. Every sound is synthesised in the browser, so there
          are no sample files. Sound starts after your first click or key press, and reduced motion
          does not mute it.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={muted}
            onChange={(e) => setMuted(e.target.checked)}
            className="size-4 accent-(--color-ink)"
          />
          Mute
        </label>
        <label className="flex items-center gap-3 text-sm">
          Volume
          <input
            type="range"
            min={0}
            max={100}
            step={5}
            value={Math.round(volume * 100)}
            onChange={(e) => setVolume(Number(e.target.value) / 100)}
            aria-valuetext={`${Math.round(volume * 100)} percent`}
            className="w-40 accent-(--color-ink)"
          />
          <span className="w-10 tabular-nums text-ink-soft">{Math.round(volume * 100)}%</span>
        </label>
      </div>
      <ul className="m-0 grid list-none grid-cols-1 gap-x-8 gap-y-2 p-0 sm:grid-cols-2">
        {SOUNDS.map((s) => (
          <li key={s.id} className="flex items-baseline justify-between gap-3">
            <button
              type="button"
              onClick={() => play(s.id)}
              className="rounded-field px-3 py-1.5 text-sm font-medium hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
            >
              Play {s.label}
            </button>
            <span className="text-sm text-ink-soft">{s.note}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
