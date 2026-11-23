"use client";

import { Button, Tabs, TabsContent, TabsList, TabsTrigger } from "@ci/ui/primitives";
import type { ReactNode } from "react";
import {
  type BoardColouring,
  COLOURINGS,
  ColouringStyles,
  PieceRow,
  type PieceSetName,
  StaticBoard,
} from "./board/static-board";
import { SwitchRow, VolumeRow } from "./shell";
import { SOUND_SETS } from "./sound-sets";

// The proposed Settings page (none exists yet): Sound, Board and Moves. Three layouts share the
// same sections; the pickers are radio groups drawn as choices with a live preview.

const START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

function Choice({
  name,
  checked,
  title,
  hint,
  children,
}: {
  name: string;
  checked: boolean;
  title: string;
  hint?: string;
  children?: ReactNode;
}) {
  return (
    <label
      className={`flex cursor-pointer flex-col gap-3 rounded-field border p-3 transition-colors duration-(--dur-quick) ease-out ${checked ? "border-ink bg-well" : "border-rule-strong hover:bg-(--surface-hover)"}`}
    >
      <span className="flex items-start gap-2">
        <input
          type="radio"
          name={name}
          defaultChecked={checked}
          className="mt-1 accent-(--color-ink)"
        />
        <span className="flex flex-col gap-0.5">
          <span className={`text-sm text-ink ${checked ? "font-semibold" : ""}`}>{title}</span>
          {hint ? <span className="text-xs text-ink-soft">{hint}</span> : null}
        </span>
      </span>
      {children}
    </label>
  );
}

function Group({ legend, note, children }: { legend: string; note?: string; children: ReactNode }) {
  return (
    <fieldset className="m-0 flex min-w-0 flex-col gap-3 border-0 p-0">
      <legend className="mb-1 p-0 text-base font-medium text-ink">{legend}</legend>
      {note ? <p className="m-0 -mt-1 max-w-prose text-sm text-ink-soft">{note}</p> : null}
      {children}
    </fieldset>
  );
}

export function SoundSection() {
  return (
    <div className="flex flex-col gap-6">
      <div className="flex max-w-md flex-col gap-4">
        <SwitchRow label="Board sounds" hint="Moves, captures, checks and drill results" on />
        <VolumeRow value={60} />
        <SwitchRow label="Pen scratch" hint="A faint scratch as a red-pen mark draws" on={false} />
      </div>
      <Group legend="Sound set" note="Hear each one before you choose.">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          {SOUND_SETS.map((s, i) => (
            <Choice
              key={s.id}
              name="sound-set"
              checked={i === 0}
              title={s.name}
              hint={s.description.split(".")[0]}
            >
              <Button variant="outline" size="sm" className="w-fit">
                Play a move
              </Button>
            </Choice>
          ))}
        </div>
      </Group>
      <p className="m-0 max-w-prose text-sm text-ink-soft">
        Sound follows this setting only; reduced motion does not mute it. Every sound has a visual
        equivalent on the board.
      </p>
    </div>
  );
}

export function BoardSection({
  set = "cburnett",
  colouring = "slate",
}: {
  set?: PieceSetName;
  colouring?: BoardColouring;
}) {
  return (
    <div className="flex flex-col gap-8">
      <ColouringStyles />
      <Group legend="Pieces" note="Applies to every board in the app.">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <Choice
            name="pieces"
            checked={set === "cburnett"}
            title="Cburnett"
            hint="The default: flat outlines, like a printed diagram"
          >
            <PieceRow set="cburnett" colouring={colouring} sq={34} />
          </Choice>
          <Choice
            name="pieces"
            checked={set === "chessnut"}
            title="Chessnut"
            hint="Rounder and heavier, the same two inks"
          >
            <PieceRow set="chessnut" colouring={colouring} sq={34} />
          </Choice>
        </div>
      </Group>
      <Group legend="Squares" note="Three colourings, each made from the palette's own colours.">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          {(Object.keys(COLOURINGS) as BoardColouring[]).map((c) => (
            <Choice
              key={c}
              name="squares"
              checked={c === colouring}
              title={COLOURINGS[c].name}
              hint={COLOURINGS[c].note}
            >
              <StaticBoard
                fen={START}
                set={set}
                colouring={c}
                px={160}
                label={`${COLOURINGS[c].name} squares`}
              />
            </Choice>
          ))}
        </div>
      </Group>
      <SwitchRow label="Coordinates" hint="Files and ranks on the edge squares" on />
    </div>
  );
}

export function MovesSection() {
  return (
    <div className="flex flex-col gap-8">
      <Group legend="Promotion" note="What happens when your pawn reaches the last rank.">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <Choice
            name="promo"
            checked
            title="Ask every time"
            hint="Pick the piece from four on the board"
          />
          <Choice
            name="promo"
            checked={false}
            title="Always a queen"
            hint="Hold the pawn on the square to choose instead"
          />
          <Choice
            name="promo"
            checked={false}
            title="Queen, ask in drills"
            hint="A drill can need a knight; the trainer always asks"
          />
        </div>
      </Group>
      <div className="flex max-w-md flex-col gap-4">
        <SwitchRow
          label="Premoves"
          hint="In a play-out, queue your next move during the reply; it plays when it is your turn"
          on={false}
        />
        <SwitchRow
          label="Show legal moves"
          hint="Dots on the squares a picked-up piece can reach"
          on
        />
        <SwitchRow label="Ask how sure I am" hint="In the trainer, before the answer" on />
      </div>
    </div>
  );
}

const SECTIONS = [
  { id: "sound", title: "Sound", body: <SoundSection /> },
  { id: "board", title: "Board", body: <BoardSection /> },
  { id: "moves", title: "Moves", body: <MovesSection /> },
];

function PageHead() {
  return (
    <div className="flex flex-col gap-2">
      <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Settings</h1>
      <p className="m-0 max-w-prose text-prose text-ink-soft">
        Saved to your account, so every device you sign in on looks and sounds the same.
      </p>
    </div>
  );
}

/** A: one long page, a sheet per section. */
export function SettingsLong() {
  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-8 px-(--space-gutter) py-10">
      <PageHead />
      {SECTIONS.map((s) => (
        <section
          key={s.id}
          aria-labelledby={`s-${s.id}`}
          data-surface="sheet"
          className="flex flex-col gap-5 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
        >
          <h2 id={`s-${s.id}`} className="m-0 font-serif text-xl font-semibold text-ink">
            {s.title}
          </h2>
          {s.body}
        </section>
      ))}
    </div>
  );
}

/** B: a section list beside the content (a row of links on phones). */
export function SettingsSideNav({ active = "sound" }: { active?: string }) {
  const current = SECTIONS.find((s) => s.id === active) ?? SECTIONS[0];
  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-8 px-(--space-gutter) py-10">
      <PageHead />
      <div className="flex flex-col gap-6 lg:flex-row lg:gap-12">
        <nav aria-label="Settings sections" className="shrink-0 lg:w-48">
          <ul className="m-0 flex list-none gap-1 p-0 lg:flex-col">
            {SECTIONS.map((s) => (
              <li key={s.id}>
                <span
                  aria-current={s.id === current?.id ? "page" : undefined}
                  className={`flex h-10 items-center rounded-field px-3 text-sm ${s.id === current?.id ? "bg-well font-semibold text-ink" : "text-ink-soft"}`}
                >
                  {s.title}
                </span>
              </li>
            ))}
          </ul>
        </nav>
        <section
          data-surface="sheet"
          className="flex min-w-0 flex-1 flex-col gap-5 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
        >
          <h2 className="m-0 font-serif text-xl font-semibold text-ink">{current?.title}</h2>
          {current?.body}
        </section>
      </div>
    </div>
  );
}

/** C: tabs, one section at a time, opened from the account menu. */
export function SettingsTabs({ active = "board" }: { active?: string }) {
  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-8 px-(--space-gutter) py-10">
      <PageHead />
      <Tabs defaultValue={active}>
        <TabsList>
          {SECTIONS.map((s) => (
            <TabsTrigger key={s.id} value={s.id}>
              {s.title}
            </TabsTrigger>
          ))}
        </TabsList>
        {SECTIONS.map((s) => (
          <TabsContent key={s.id} value={s.id}>
            <div
              data-surface="sheet"
              className="mt-6 flex flex-col gap-5 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
            >
              {s.body}
            </div>
          </TabsContent>
        ))}
      </Tabs>
    </div>
  );
}
