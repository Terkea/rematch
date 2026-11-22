"use client";

import { Select as SelectPrimitive } from "@base-ui/react/select";
import { CaretDownIcon, CheckIcon } from "@phosphor-icons/react/dist/ssr";
import { Icon } from "../icons/icon.js";
import { cn } from "../lib/cn.js";
import { overlay } from "../motion/index.js";

export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps {
  label: string;
  value: string;
  options: SelectOption[];
  onValueChange: (value: string) => void;
  /** Submits the value with a form under this name. */
  name?: string;
  className?: string;
}

// Base UI Select restyled to the scoresheet tokens. The trigger is an input-like `well` with a
// `rule-strong` border (it must be seen to be operated); the list floats on `sheet` with
// `--shadow-lift`, the one shadow, and grows from the trigger over `--dur-quick` (opacity and
// scale only, and only opacity under reduced motion). The chosen option is `ink` 600 with a check,
// so the selection never rests on colour alone.
export function Select({ label, value, options, onValueChange, name, className }: SelectProps) {
  return (
    <SelectPrimitive.Root
      value={value}
      items={options}
      name={name}
      onValueChange={(v) => {
        if (typeof v === "string") onValueChange(v);
      }}
    >
      <div data-slot="select" className={cn("flex min-w-0 flex-col gap-1.5", className)}>
        <SelectPrimitive.Label className="w-fit font-sans text-sm font-medium text-ink">
          {label}
        </SelectPrimitive.Label>
        <SelectPrimitive.Trigger
          data-slot="select-trigger"
          className={cn(
            "flex h-10 w-full min-w-0 items-center justify-between gap-2 rounded-field border border-rule-strong bg-well px-3",
            "text-left font-sans text-base text-ink",
            "transition-colors duration-(--dur-quick) ease-out hover:bg-(--surface-hover)",
            "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
            "data-popup-open:bg-sheet",
          )}
        >
          <SelectPrimitive.Value className="truncate" />
          <SelectPrimitive.Icon className="shrink-0 text-ink-soft transition-transform duration-(--dur-quick) ease-out data-popup-open:rotate-180 motion-reduce:transition-none">
            <Icon icon={CaretDownIcon} size={16} />
          </SelectPrimitive.Icon>
        </SelectPrimitive.Trigger>
      </div>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Positioner
          sideOffset={overlay.floatOffset}
          alignItemWithTrigger={false}
          className="z-50 outline-none"
        >
          <SelectPrimitive.Popup
            data-slot="select-popup"
            className={cn(
              "min-w-(--anchor-width) rounded-field border border-rule bg-sheet p-1 shadow-lift outline-none",
              "origin-(--transform-origin) transition-[opacity,scale] duration-(--dur-quick) ease-out",
              "data-starting-style:scale-(--enter-float-scale) data-starting-style:opacity-0",
              "data-ending-style:scale-(--enter-float-scale) data-ending-style:opacity-0",
              "motion-reduce:data-starting-style:scale-100 motion-reduce:data-ending-style:scale-100",
            )}
          >
            <SelectPrimitive.List className="max-h-(--available-height) overflow-y-auto">
              {options.map((o) => (
                <SelectPrimitive.Item
                  key={o.value}
                  value={o.value}
                  className={cn(
                    "flex min-h-10 cursor-default items-center gap-2 rounded-field py-2 pr-4 pl-2",
                    "font-sans text-base text-ink-soft outline-none select-none",
                    "data-highlighted:bg-well data-highlighted:text-ink",
                    "data-selected:font-semibold data-selected:text-ink",
                    "pointer-coarse:min-h-11",
                  )}
                >
                  <span className="flex size-4 shrink-0 items-center text-ink">
                    <SelectPrimitive.ItemIndicator>
                      <Icon icon={CheckIcon} size={16} />
                    </SelectPrimitive.ItemIndicator>
                  </span>
                  <SelectPrimitive.ItemText>{o.label}</SelectPrimitive.ItemText>
                </SelectPrimitive.Item>
              ))}
            </SelectPrimitive.List>
          </SelectPrimitive.Popup>
        </SelectPrimitive.Positioner>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
