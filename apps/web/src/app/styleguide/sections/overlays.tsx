"use client";

import {
  CaretRightIcon,
  CheckCircleIcon,
  Icon,
  InfoIcon,
  KeyboardIcon,
  TrashIcon,
} from "@ci/ui/icons";
import { CommandMenu, type CommandMenuItem, Drawer, toast } from "@ci/ui/overlays";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Kbd,
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverTitle,
  PopoverTrigger,
  SegmentedControl,
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@ci/ui/primitives";
import { type ReactNode, useMemo, useState } from "react";

// What floats. The tooltip and the popover are pinned open in place, so their tone, radius and the
// one shadow can be read without a pointer. The dialog, drawer, command menu and toasts take over
// the screen, so each opens from its own button, with the app's own copy.

const linking = [
  { title: "Your chess.com username", detail: "We check the account exists." },
  { title: "Put the code in Location", detail: "On your chess.com profile. Remove it afterwards." },
  { title: "Check again", detail: "The account is yours once chess.com shows the code." },
];

function Specimen({
  title,
  note,
  children,
  className = "",
}: {
  title: string;
  note: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      data-surface="sheet"
      className={`flex min-w-0 flex-col gap-4 rounded-sheet bg-sheet p-(--space-panel-pad) ${className}`}
    >
      <div>
        <h3 className="font-serif text-xl font-semibold">{title}</h3>
        <p className="mt-1 max-w-prose text-sm text-ink-soft">{note}</p>
      </div>
      {children}
    </div>
  );
}

function Opener({ term, detail, children }: { term: string; detail: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <dt className="font-medium">
        {term} <span className="font-normal text-ink-soft">{detail}</span>
      </dt>
      <dd>{children}</dd>
    </div>
  );
}

function PinnedTooltip() {
  return (
    <TooltipProvider>
      {/* Pinned: opening and closing are ignored, so it stays put for reading. */}
      <Tooltip open onOpenChange={() => {}}>
        <TooltipTrigger
          render={<Button variant="outline" size="sm" aria-label="Next move" tabIndex={-1} />}
        >
          <Icon icon={CaretRightIcon} />
        </TooltipTrigger>
        <TooltipContent side="right">
          Next move
          <Kbd>
            <Icon icon={CaretRightIcon} size={16} label="Right arrow" />
          </Kbd>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

function PinnedPopover() {
  return (
    <Popover open onOpenChange={() => {}} modal={false}>
      <PopoverTrigger render={<Button variant="quiet" size="sm" tabIndex={-1} />} nativeButton>
        <Icon icon={InfoIcon} />
        How linking works
      </PopoverTrigger>
      <PopoverContent initialFocus={false} finalFocus={false}>
        <PopoverTitle>How linking works</PopoverTitle>
        <ol className="flex flex-col gap-2">
          {linking.map((step, i) => (
            <li key={step.title} className="flex gap-3">
              <span className="w-3 shrink-0 font-serif font-semibold tabular-nums lining-nums text-ink-faint">
                {i + 1}
              </span>
              <PopoverDescription render={<span />} className="text-ink-soft">
                <span className="block font-medium text-ink">{step.title}</span>
                {step.detail}
              </PopoverDescription>
            </li>
          ))}
        </ol>
      </PopoverContent>
    </Popover>
  );
}

function DeleteDialog() {
  return (
    <Dialog>
      <DialogTrigger render={<Button variant="danger" />}>
        <Icon icon={TrashIcon} />
        Delete account
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete your account?</DialogTitle>
          <DialogDescription>
            Your linked accounts, imported games and analysis are removed for good. Your games stay
            on chess.com and Lichess.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogClose render={<Button variant="outline" />}>Keep my account</DialogClose>
          <DialogClose render={<Button variant="danger" />}>Delete account</DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

const timeControls = [
  { value: "all", label: "All" },
  { value: "bullet", label: "Bullet" },
  { value: "blitz", label: "Blitz" },
  { value: "rapid", label: "Rapid" },
];

function FiltersDrawer() {
  const [time, setTime] = useState("blitz");
  return (
    <Drawer.Root>
      <Drawer.Trigger asChild>
        <Button variant="outline">Filters</Button>
      </Drawer.Trigger>
      <Drawer.Content>
        <Drawer.Title>Filters</Drawer.Title>
        <Drawer.Description>Narrow your games before you review them.</Drawer.Description>
        <div className="mt-4">
          <SegmentedControl
            label="Time control"
            value={time}
            options={timeControls}
            onValueChange={setTime}
          />
        </div>
      </Drawer.Content>
    </Drawer.Root>
  );
}

function Menu() {
  const [open, setOpen] = useState(false);
  const [last, setLast] = useState<string | null>(null);
  const items = useMemo<CommandMenuItem[]>(
    () =>
      [
        ["Go to overview", "Pages", ["home", "report"]],
        ["Go to games", "Pages", ["review", "list"]],
        ["Go to accounts", "Pages", ["chess.com", "lichess", "link"]],
        ["Switch to night study", "Preferences", ["dark", "theme"]],
        ["Sign out", "Account", ["log out"]],
      ].map(([label, group, keywords]) => ({
        id: label as string,
        label: label as string,
        group: group as string,
        keywords: keywords as string[],
        onSelect: () => setLast(label as string),
      })),
    [],
  );
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <Button variant="outline" onClick={() => setOpen(true)}>
        <Icon icon={KeyboardIcon} />
        Search pages and actions
      </Button>
      <p className="text-sm text-ink-soft">
        Or press <Kbd>Ctrl</Kbd> <Kbd>K</Kbd>.{" "}
        <span aria-live="polite">{last ? `Chose “${last}”.` : ""}</span>
      </p>
      <CommandMenu items={items} open={open} onOpenChange={setOpen} />
    </div>
  );
}

function Toasts() {
  return (
    <div className="flex flex-wrap gap-3">
      <Button variant="outline" onClick={() => toast.success("chess.com connected")}>
        <Icon icon={CheckCircleIcon} />
        Show a success
      </Button>
      <Button
        variant="outline"
        onClick={() => toast.error("Lichess wasn't connected. You can try again.")}
      >
        Show an error
      </Button>
      <Button variant="quiet" onClick={() => toast("Code copied")}>
        Show a note
      </Button>
    </div>
  );
}

export function OverlaysSection() {
  return (
    <section
      id="overlays"
      data-section="overlays"
      aria-labelledby="overlays-title"
      className="flex flex-col gap-6"
    >
      <div className="max-w-prose">
        <h2 id="overlays-title" className="font-serif text-2xl font-semibold">
          Overlays
        </h2>
        <p className="mt-2 text-ink-soft">
          Only what floats casts the one shadow, and it grows from where it came from. Nothing blurs
          the page behind it.
        </p>
      </div>
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <Specimen
          title="Tooltip"
          note="An ink label, no arrow, after the pointer rests. A shortcut sits in it as a key cap."
        >
          <div className="min-h-10">
            <PinnedTooltip />
          </div>
        </Specimen>
        <Specimen
          title="Popover"
          note="A sheet with the field radius, for a short explanation beside its trigger."
          className="lg:row-span-2"
        >
          <div className="min-h-72">
            <PinnedPopover />
          </div>
        </Specimen>
        <Specimen
          title="Over the page"
          note="These take the screen, so each opens from its own control. The page dims to paper behind them; nothing blurs."
        >
          <dl className="flex flex-col gap-4">
            <Opener term="Dialog" detail="A decision that can't be undone, named by its verb.">
              <DeleteDialog />
            </Opener>
            <Opener term="Drawer" detail="Filters and panels rise from the bottom edge on a phone.">
              <FiltersDrawer />
            </Opener>
            <Opener term="Command menu" detail="Every page and action, by name or keyword.">
              <Menu />
            </Opener>
            <Opener
              term="Toasts"
              detail="Bottom centre on a phone, bottom right above 640px. An error is announced at once."
            >
              <Toasts />
            </Opener>
          </dl>
        </Specimen>
      </div>
    </section>
  );
}
