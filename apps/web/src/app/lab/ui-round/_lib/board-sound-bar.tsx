"use client";

import { ArrowsClockwiseIcon, GearSixIcon, Icon, SpeakerHighIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import { FloatPanel, SwitchRow, VolumeRow } from "./shell";

/** Option B's board toolbar: flip and sound, with the sound panel open above it. */
export function BoardSoundBar({ open = false }: { open?: boolean }) {
  return (
    <div className="relative flex items-center gap-2">
      <Button variant="quiet" size="sm">
        <Icon icon={ArrowsClockwiseIcon} />
        Flip board
      </Button>
      <div>
        <Button variant="quiet" size="sm" aria-pressed className={open ? "bg-sheet" : ""}>
          <Icon icon={SpeakerHighIcon} />
          Sound on
        </Button>
        {open ? (
          <div className="absolute bottom-full left-0 z-50 mb-1.5">
            <FloatPanel>
              <SwitchRow label="Board sounds" on />
              <VolumeRow value={60} />
              <span className="flex items-center gap-2 border-t border-rule pt-3 text-sm text-ink underline decoration-rule-strong underline-offset-4">
                <Icon icon={GearSixIcon} />
                Sound set and more in Settings
              </span>
            </FloatPanel>
          </div>
        ) : null}
      </div>
    </div>
  );
}
