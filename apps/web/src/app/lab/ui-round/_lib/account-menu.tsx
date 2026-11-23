"use client";

import { GearSixIcon, Icon, SignOutIcon, SpeakerHighIcon } from "@ci/ui/icons";
import { Switch } from "@ci/ui/primitives";
import { FloatPanel } from "./shell";

/** Option C: the account menu with a Sound row and a Settings link. */
export function AccountMenuPanel() {
  const row = "flex h-10 items-center gap-2 rounded-field px-2 text-sm text-ink-soft";
  return (
    <FloatPanel>
      <div className="flex flex-col gap-0.5">
        <span className="text-xs text-ink-faint">Signed in as</span>
        <span className="truncate text-sm font-medium text-ink">demo.player@example.com</span>
      </div>
      <div className="-mx-2 flex flex-col border-t border-rule pt-2">
        <div className="flex h-10 items-center justify-between gap-2 rounded-field px-2 text-sm text-ink">
          <span className="flex items-center gap-2">
            <Icon icon={SpeakerHighIcon} />
            Sound
          </span>
          <Switch defaultChecked aria-label="Sound" />
        </div>
        <div className="flex items-center gap-3 px-2 pb-2">
          <span className="text-xs text-ink-soft">Volume</span>
          <input
            type="range"
            min={0}
            max={100}
            defaultValue={60}
            aria-label="Volume"
            className="h-6 flex-1 accent-(--color-ink)"
          />
          <span className="text-xs text-ink-soft [font-variant-numeric:lining-nums_tabular-nums]">
            60%
          </span>
        </div>
        <span className={`${row} bg-(--surface-hover) text-ink`}>
          <Icon icon={GearSixIcon} />
          Settings
        </span>
        <span className={row}>
          <Icon icon={SignOutIcon} />
          Sign out
        </span>
      </div>
    </FloatPanel>
  );
}
