"use client";

import { ArrowsClockwiseIcon, Icon, TrashIcon } from "@ci/ui/icons";
import {
  Button,
  Field,
  Input,
  SegmentedControl,
  Select,
  Switch,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@ci/ui/primitives";
import type { ReactNode } from "react";
import { Forced, NotUsed, StateGrid, type StateKey, type StateRow } from "./state-grid";

// Every primitive in its five states, with the app's own copy. Hover and focus-visible are forced
// from the components' CSS, so a cell can't show a look the component doesn't have. Where a state
// has no look of its own, the cell says why instead of inventing one.

function force(state: StateKey, node: ReactNode, target?: string) {
  if (state === "hover")
    return (
      <Forced state="hover" target={target}>
        {node}
      </Forced>
    );
  if (state === "focus")
    return (
      <Forced state="focus-visible" target={target}>
        {node}
      </Forced>
    );
  return node;
}

/** Only the rest column is in the tab order; the other cells are specimens. */
const specimen = (state: StateKey) => (state === "rest" ? undefined : -1);

const buttonRow = (
  name: string,
  variant: "primary" | "outline" | "quiet" | "danger",
  label: string,
  icon?: typeof TrashIcon,
  size: "sm" | "md" = "md",
): StateRow => ({
  name,
  render: (state) =>
    state === "error" ? (
      <NotUsed />
    ) : (
      force(
        state,
        <Button
          variant={variant}
          size={size}
          disabled={state === "disabled"}
          tabIndex={specimen(state)}
        >
          {icon ? <Icon icon={icon} /> : null}
          {label}
        </Button>,
      )
    ),
});

const buttons: StateRow[] = [
  buttonRow("Primary", "primary", "Import games"),
  buttonRow("Outline", "outline", "Review game"),
  buttonRow("Quiet", "quiet", "Check again", ArrowsClockwiseIcon),
  buttonRow("Danger", "danger", "Delete account", TrashIcon),
  buttonRow("Small", "outline", "Replay marks", undefined, "sm"),
];

const timeControls = [
  { value: "bullet", label: "Bullet" },
  { value: "blitz", label: "Blitz" },
  { value: "rapid", label: "Rapid" },
];
const colours = [
  { value: "white", label: "White" },
  { value: "black", label: "Black" },
];
const noop = () => {};

const fields: StateRow[] = [
  {
    name: "Text field",
    render: (state) =>
      force(
        state,
        <Field
          label="chess.com username"
          error={state === "error" ? "Enter your chess.com username." : undefined}
        >
          <Input
            readOnly={state !== "rest"}
            tabIndex={specimen(state)}
            autoComplete="off"
            disabled={state === "disabled"}
            defaultValue={state === "error" ? "" : "Night-King96"}
          />
        </Field>,
        "input",
      ),
  },
  {
    name: "Select",
    render: (state) =>
      state === "disabled" || state === "error" ? (
        <NotUsed>
          {state === "disabled"
            ? "Not offered: a filter with one choice is hidden, not greyed."
            : "No error look: every option is valid."}
        </NotUsed>
      ) : (
        force(
          state,
          <Select label="Time control" value="blitz" options={timeControls} onValueChange={noop} />,
          "[data-slot='select-trigger']",
        )
      ),
  },
];

const toggles: StateRow[] = [
  {
    name: "Switch",
    render: (state) =>
      state === "error" ? (
        <NotUsed>No error look: on and off are both valid.</NotUsed>
      ) : (
        force(
          state,
          <span className="flex items-center gap-3">
            <Switch
              aria-label="Move sounds"
              defaultChecked
              disabled={state === "disabled"}
              tabIndex={specimen(state)}
            />
            <span className={state === "disabled" ? "text-sm text-ink-faint" : "text-sm"}>
              Move sounds
            </span>
          </span>,
        )
      ),
  },
  {
    name: "Tabs",
    render: (state) =>
      state === "error" ? (
        <NotUsed>No error look: a tab is never wrong.</NotUsed>
      ) : (
        force(
          state,
          <Tabs defaultValue="moves">
            <TabsList aria-label={`Game panels, ${state}`}>
              <TabsTrigger value="moves">Moves</TabsTrigger>
              <TabsTrigger
                value="mistakes"
                disabled={state === "disabled"}
                tabIndex={specimen(state)}
              >
                Mistakes
              </TabsTrigger>
            </TabsList>
          </Tabs>,
          "[data-slot='tabs-trigger']:not([data-active])",
        )
      ),
  },
  {
    name: "Segmented",
    render: (state) =>
      state === "disabled" || state === "error" ? (
        <NotUsed>
          {state === "disabled"
            ? "Not offered: every choice always applies."
            : "No error look: one choice is always made."}
        </NotUsed>
      ) : (
        force(
          state,
          <SegmentedControl label="Colour" value="white" options={colours} onValueChange={noop} />,
          "[role='radio']:not([data-checked])",
        )
      ),
  },
];

function Family({ title, note, rows }: { title: string; note: ReactNode; rows: StateRow[] }) {
  return (
    <div
      data-surface="sheet"
      className="flex flex-col gap-4 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="max-w-prose">
        <h3 className="font-serif text-xl font-semibold">{title}</h3>
        <p className="mt-1 text-sm text-ink-soft">{note}</p>
      </div>
      <StateGrid rows={rows} />
    </div>
  );
}

export function PrimitivesSection() {
  return (
    <section
      id="primitives"
      data-section="primitives"
      aria-labelledby="primitives-title"
      className="flex flex-col gap-6"
    >
      <div className="max-w-prose">
        <h2 id="primitives-title" className="font-serif text-2xl font-semibold">
          Primitives
        </h2>
        <p className="mt-2 text-ink-soft">
          Hover changes tone, never position: a surface steps along the tonal ladder, and a control
          that is already a well steps its edge. Focus is a 2px ink ring on every surface. Disabled
          drops to faint ink; only the primary button keeps a quiet well fill, so the main action
          keeps its place. The hover and focus cells are the components' own styles, forced on, not
          drawings of them.
        </p>
      </div>
      <Family
        title="Buttons"
        note="Destructive actions are ink-outlined with an explicit verb. A button has no error look: the field it submits carries the error, and red stays for mistakes."
        rows={buttons}
      />
      <Family
        title="Fields"
        note="A field is an inset well with a 3:1 edge. An error turns the edge pen and says what to do, under the field."
        rows={fields}
      />
      <Family
        title="Toggles and tracks"
        note="The chosen tab or option is a sheet chip in 600 ink that travels on the page spring; the track never moves."
        rows={toggles}
      />
    </section>
  );
}
