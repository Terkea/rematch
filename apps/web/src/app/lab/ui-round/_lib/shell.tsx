"use client";

import {
  GearSixIcon,
  Icon,
  MagnifyingGlassIcon,
  MoonIcon,
  SpeakerHighIcon,
  SpeakerSlashIcon,
  SunIcon,
} from "@ci/ui/icons";
import { Button, Kbd, Switch } from "@ci/ui/primitives";
import { useTheme } from "next-themes";
import type { ReactNode } from "react";
import { BrandLockup } from "../../../../components/brand/lockup";
import { navItems, onPhoneRow } from "../../../(app)/_shell/nav";

// A still copy of the app header for the mockups (the real one needs a session). Same classes as
// `(app)/_shell/header.tsx`; `sound` decides where the sound control sits, and `open` draws its
// panel open, so a screenshot shows the control in use.

const navLink =
  "relative flex h-10 items-center rounded-field px-3 text-sm font-medium whitespace-nowrap";

export type SoundPlace = "header" | "menu" | "none";

export function MockHeader({
  current = "/app/train",
  sound,
  open = false,
  muted = false,
  panel,
}: {
  current?: string;
  sound: SoundPlace;
  open?: boolean;
  muted?: boolean;
  panel?: ReactNode;
}) {
  const { resolvedTheme } = useTheme();
  const dark = resolvedTheme === "dark";
  return (
    <header className="relative z-40 bg-paper">
      <div className="mx-auto flex max-w-6xl flex-wrap items-center gap-x-4 px-(--space-gutter) pt-3 md:h-16 md:flex-nowrap md:gap-x-6 md:pt-0">
        <span className="flex h-10 shrink-0 items-center text-ink">
          <BrandLockup />
        </span>
        <nav
          aria-label="Main"
          className="order-last -mx-3 shrink-0 basis-full md:order-none md:mx-0 md:basis-auto"
        >
          <ul className="flex items-center gap-1">
            {navItems.map((item) => {
              const on = item.href === current;
              return (
                <li key={item.href} className={onPhoneRow(item.href) ? undefined : "max-lg:hidden"}>
                  <span className={`${navLink} ${on ? "font-semibold text-ink" : "text-ink-soft"}`}>
                    {item.label}
                    {on ? (
                      <span className="absolute inset-x-3 bottom-0 h-0.5 rounded-pill bg-ink" />
                    ) : null}
                  </span>
                </li>
              );
            })}
          </ul>
        </nav>
        <div className="flex min-w-0 flex-1 basis-0 items-center justify-end gap-2">
          <Button variant="quiet" size="sm" className="max-sm:w-8 max-sm:px-0">
            <Icon icon={MagnifyingGlassIcon} />
            <span className="max-sm:sr-only md:sr-only">Search</span>
            <Kbd className="max-sm:hidden">Ctrl K</Kbd>
          </Button>
          {sound === "header" ? (
            <div className="relative">
              <Button
                variant="quiet"
                size="sm"
                aria-label={muted ? "Sound off" : "Sound"}
                aria-expanded={open}
                className={`w-8 px-0 ${open ? "bg-sheet" : ""}`}
              >
                <Icon icon={muted ? SpeakerSlashIcon : SpeakerHighIcon} />
              </Button>
              {open && panel ? (
                <div className="absolute top-full right-0 mt-1.5 max-sm:-right-20">{panel}</div>
              ) : null}
            </div>
          ) : null}
          <Button variant="quiet" size="sm" aria-label="Theme" className="w-8 px-0">
            <Icon icon={dark ? SunIcon : MoonIcon} />
          </Button>
          <div className="relative">
            <span className="relative flex size-8 items-center justify-center rounded-pill bg-well font-serif text-base font-semibold text-ink">
              D
            </span>
            {sound === "menu" && open && panel ? (
              <div className="absolute top-full right-0 mt-1.5">{panel}</div>
            ) : null}
          </div>
        </div>
      </div>
    </header>
  );
}

/** The floating surface every popover uses (sheet, field radius, the one shadow). */
export function FloatPanel({ children, wide = false }: { children: ReactNode; wide?: boolean }) {
  return (
    <div
      data-surface="sheet"
      className={`flex ${wide ? "w-80" : "w-72"} max-w-[calc(100vw-2*var(--space-gutter))] flex-col gap-3 rounded-field border border-rule bg-sheet p-4 text-sm text-ink shadow-lift dark:border-rule-strong`}
    >
      {children}
    </div>
  );
}

/** A volume row: a native range in ink, with its value said in words beside it. */
export function VolumeRow({ value, label = "Volume" }: { value: number; label?: string }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="flex items-baseline justify-between text-sm text-ink">
        <span>{label}</span>
        <span className="text-ink-soft [font-variant-numeric:lining-nums_tabular-nums]">
          {value}%
        </span>
      </span>
      <input
        type="range"
        min={0}
        max={100}
        defaultValue={value}
        className="h-6 w-full cursor-pointer accent-(--color-ink)"
      />
    </label>
  );
}

export function SwitchRow({ label, hint, on }: { label: string; hint?: string; on: boolean }) {
  return (
    <div className="flex items-start justify-between gap-4">
      <span className="flex flex-col gap-0.5">
        <span className="text-sm text-ink">{label}</span>
        {hint ? <span className="text-xs text-ink-soft">{hint}</span> : null}
      </span>
      <Switch defaultChecked={on} aria-label={label} />
    </div>
  );
}

/** The sound popover's body, shared by the header and the account-menu placements. */
export function SoundPanelBody({ settingsLink = true }: { settingsLink?: boolean }) {
  return (
    <>
      <SwitchRow label="Board sounds" on />
      <VolumeRow value={60} />
      <SwitchRow label="Pen scratch" hint="A faint scratch as a red-pen mark draws" on={false} />
      <p className="m-0 text-xs text-ink-soft">
        Sound never says anything the board does not show.
      </p>
      {settingsLink ? (
        <span className="flex items-center gap-2 border-t border-rule pt-3 text-sm text-ink underline decoration-rule-strong underline-offset-4">
          <Icon icon={GearSixIcon} />
          All settings
        </span>
      ) : null}
    </>
  );
}
