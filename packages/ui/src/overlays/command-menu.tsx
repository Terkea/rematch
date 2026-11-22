"use client";

import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { Command } from "cmdk";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../icons/icon.js";
import { MagnifyingGlassIcon } from "../icons/index.js";
import { Kbd } from "../primitives/kbd.js";

// A cmdk list inside a Base UI dialog (focus trap, Escape, focus return and inert page come with
// it). A `sheet` panel on a `paper` wash, `--radius-sheet`, lifted by the one shadow. It rises
// 8px over `--dur-base` and only fades under reduced motion (M9, M10). Ctrl+K and Cmd+K open it.

export interface CommandMenuItem {
  id: string;
  label: string;
  group: string;
  keywords?: string[];
  onSelect: () => void;
}

export interface CommandMenuProps {
  items: CommandMenuItem[];
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}

const row = [
  "relative flex min-h-10 cursor-default items-center rounded-field px-3 font-sans text-base text-ink select-none",
  "pointer-coarse:min-h-11 data-[selected=true]:bg-well data-[selected=true]:font-semibold",
].join(" ");

const popup = [
  "fixed top-[12vh] left-1/2 z-50 w-[calc(100%-2*var(--space-gutter))] max-w-lg -translate-x-1/2 overflow-hidden",
  "rounded-sheet border border-rule bg-sheet text-ink shadow-lift outline-none dark:border-rule-strong",
  "origin-top transition-[opacity,scale,translate] duration-(--dur-base) ease-out",
  "data-starting-style:translate-y-(--enter-dialog-y) data-starting-style:scale-(--enter-dialog-scale) data-starting-style:opacity-0",
  "data-ending-style:scale-(--enter-dialog-scale) data-ending-style:opacity-0 data-ending-style:duration-(--dur-quick)",
  "motion-reduce:data-starting-style:translate-y-0 motion-reduce:data-starting-style:scale-100",
  "motion-reduce:data-ending-style:scale-100 motion-reduce:duration-(--dur-quick)",
].join(" ");

const heading = [
  "[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:pb-1",
  "[&_[cmdk-group-heading]]:font-sans [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:text-ink-faint",
].join(" ");

function isEditable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      target.closest("input, textarea, select, [contenteditable]:not([contenteditable=false])") !==
        null)
  );
}

/**
 * Opens on Ctrl+K and Cmd+K from anywhere except a text field, textarea, select or contenteditable
 * (unless it is already open), so typing shortcuts are never hijacked.
 */
export function CommandMenu({ items, open: openProp, onOpenChange }: CommandMenuProps) {
  const [inner, setInner] = useState(false);
  const controlled = openProp !== undefined;
  const open = controlled ? openProp : inner;
  const setOpen = useCallback(
    (next: boolean) => {
      if (!controlled) setInner(next);
      onOpenChange?.(next);
    },
    [controlled, onOpenChange],
  );

  // There is no trigger element (Ctrl+K opens it from anywhere), so remember where focus was.
  const returnTo = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (open) {
      if (!returnTo.current && document.activeElement instanceof HTMLElement) {
        returnTo.current = document.activeElement;
      }
    } else if (returnTo.current) {
      returnTo.current.focus();
      returnTo.current = null;
    }
  }, [open]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        // Typing in a field keeps Ctrl+K, unless the menu is already open.
        if (!open && isEditable(event.target)) return;
        event.preventDefault();
        setOpen(!open);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  const groups = useMemo(() => {
    const map = new Map<string, CommandMenuItem[]>();
    for (const item of items) map.set(item.group, [...(map.get(item.group) ?? []), item]);
    return [...map];
  }, [items]);
  const labels = useMemo(() => new Map(items.map((i) => [i.id, i.label])), [items]);

  // Items are keyed by id (labels may repeat), so match the label first, then the keywords.
  const filter = useCallback(
    (id: string, search: string, keywords?: string[]) => {
      const q = search.trim().toLowerCase();
      if (!q) return 1;
      const label = (labels.get(id) ?? "").toLowerCase();
      if (label.startsWith(q)) return 1;
      if (label.includes(q)) return 0.8;
      return keywords?.some((k) => k.toLowerCase().includes(q)) ? 0.5 : 0;
    },
    [labels],
  );

  return (
    <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Backdrop
          data-slot="command-overlay"
          className="fixed inset-0 z-50 bg-paper/80 transition-opacity dark:bg-well/90 duration-(--dur-base) ease-out motion-reduce:duration-(--dur-quick) data-ending-style:opacity-0 data-starting-style:opacity-0"
        />
        <DialogPrimitive.Popup data-slot="command-menu" className={popup}>
          <DialogPrimitive.Title className="sr-only">Command menu</DialogPrimitive.Title>
          <Command label="Command menu" filter={filter} loop>
            <div className="flex items-center gap-3 border-b border-rule px-4">
              <Icon icon={MagnifyingGlassIcon} size={20} className="shrink-0 text-ink-soft" />
              <Command.Input
                autoFocus
                placeholder="Search pages and actions"
                className="h-12 min-w-0 flex-1 bg-transparent font-sans text-base text-ink outline-none placeholder:text-ink-faint"
              />
              <Kbd className="max-sm:hidden">Esc</Kbd>
            </div>
            <Command.List
              className="max-h-[min(60dvh,24rem)] overflow-y-auto overscroll-contain p-2" /* ban-lint-allow TOK2: viewport-relative cap, no token */
            >
              <Command.Empty className="px-3 py-6 text-center font-sans text-sm text-ink-soft">
                No match
              </Command.Empty>
              {groups.map(([group, list]) => (
                <Command.Group key={group} heading={group} className={heading}>
                  {list.map((item) => (
                    <Command.Item
                      key={item.id}
                      value={item.id}
                      keywords={item.keywords}
                      className={row}
                      onSelect={() => {
                        setOpen(false);
                        item.onSelect();
                      }}
                    >
                      {item.label}
                    </Command.Item>
                  ))}
                </Command.Group>
              ))}
            </Command.List>
          </Command>
        </DialogPrimitive.Popup>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
