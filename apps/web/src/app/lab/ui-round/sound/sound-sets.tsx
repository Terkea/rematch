"use client";

import { Icon, PlayIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import { useState } from "react";
import { type AuditionSound, audition, SOUND_SETS } from "../_lib/sound-sets";

const SOUNDS: { id: AuditionSound; label: string; when: string }[] = [
  { id: "move", label: "Move", when: "a piece lands" },
  { id: "capture", label: "Capture", when: "the contact frame of a capture" },
  { id: "check", label: "Check", when: "layered 20 ms after the move" },
  { id: "correct", label: "Correct", when: "a drill solved, with the good ring" },
  { id: "wrong", label: "Wrong", when: "the piece springs back" },
  { id: "gameEnd", label: "Game end", when: "mate, resignation or draw" },
];

export function SoundSets() {
  const [volume, setVolume] = useState(60);
  const [last, setLast] = useState<string | null>(null);
  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-8">
      <header className="flex max-w-prose flex-col gap-3">
        <h1 className="m-0 font-serif text-3xl font-semibold">Three sound sets</h1>
        <p className="m-0 text-prose text-ink-soft">
          Each set is the same procedural synth with different numbers: paper and wood, quiet, close
          and dry. Press a sound to hear it at the volume below. Nothing is sampled.
        </p>
        <label className="flex max-w-sm flex-col gap-1.5">
          <span className="flex items-baseline justify-between text-sm text-ink">
            <span>Volume</span>
            <span className="text-ink-soft [font-variant-numeric:lining-nums_tabular-nums]">
              {volume}%
            </span>
          </span>
          <input
            type="range"
            min={0}
            max={100}
            value={volume}
            onChange={(e) => setVolume(Number(e.target.value))}
            className="h-6 w-full cursor-pointer accent-(--color-ink)"
          />
        </label>
        <p aria-live="polite" className="m-0 min-h-5 text-sm text-ink-faint">
          {last ? `Played: ${last}` : ""}
        </p>
      </header>
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        {SOUND_SETS.map((set) => (
          <section
            key={set.id}
            aria-labelledby={`set-${set.id}`}
            data-surface="sheet"
            className="flex flex-col gap-5 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
          >
            <div className="flex flex-col gap-2">
              <p className="m-0 text-sm text-ink-soft">Set {set.id.toUpperCase()}</p>
              <h2 id={`set-${set.id}`} className="m-0 font-serif text-2xl font-semibold">
                {set.name}
              </h2>
              <p className="m-0 text-base text-ink">{set.description}</p>
            </div>
            <ul className="m-0 grid list-none grid-cols-2 gap-2 p-0">
              {SOUNDS.map((s) => (
                <li key={s.id}>
                  <Button
                    variant="outline"
                    size="sm"
                    className="w-full justify-start"
                    onClick={() => {
                      void audition(set, s.id, volume / 100);
                      setLast(`${set.name}, ${s.label.toLowerCase()}`);
                    }}
                  >
                    <Icon icon={PlayIcon} />
                    {s.label}
                  </Button>
                </li>
              ))}
            </ul>
            <p className="m-0 text-sm text-ink-soft">
              <span className="text-ink">Trade-off.</span> {set.tradeOff}
            </p>
            <details className="text-sm">
              <summary className="cursor-pointer text-ink underline decoration-rule-strong underline-offset-4">
                The numbers
              </summary>
              <dl className="mt-3 grid grid-cols-1 gap-2">
                {SOUNDS.map((s) => (
                  <div key={s.id} className="flex flex-col">
                    <dt className="text-ink">
                      {s.label} <span className="text-ink-faint">({s.when})</span>
                    </dt>
                    <dd className="m-0 text-ink-soft">{set.spec[s.id]}</dd>
                  </div>
                ))}
              </dl>
            </details>
          </section>
        ))}
      </div>
    </div>
  );
}
