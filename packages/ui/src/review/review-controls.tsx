"use client";

import { type ReactNode, useLayoutEffect, useRef } from "react";
import {
  ArrowsClockwiseIcon,
  CaretDownIcon,
  CaretLeftIcon,
  CaretRightIcon,
  CaretUpIcon,
  Icon,
  PauseIcon,
  type PhosphorIcon,
  PlayIcon,
  SkipBackIcon,
  SkipForwardIcon,
} from "../icons/index.js";
import { cn } from "../lib/index.js";
import { AnimatePresence, motion, spring, useReducedMotion } from "../motion/index.js";
import { Button, Kbd, Tooltip, TooltipContent, TooltipTrigger } from "../primitives/index.js";

export interface ReviewControlsProps {
  ply: number;
  lastPly: number;
  playing: boolean;
  /** Whether the game has judged moments to jump between. */
  hasMoments: boolean;
  onFirst: () => void;
  onPrev: () => void;
  onPlayToggle: () => void;
  onNext: () => void;
  onLast: () => void;
  onFlip: () => void;
  onPrevMoment: () => void;
  onNextMoment: () => void;
  className?: string;
}

type Key = "first" | "prev" | "play" | "next" | "last" | "prevMoment" | "nextMoment" | "flip";

/** The key cap shown in each tooltip, and the `aria-keyshortcuts` value (WAI-ARIA key names). */
const KEYS: Record<Key, { aria: string; cap: ReactNode }> = {
  first: { aria: "Home", cap: "Home" },
  prev: { aria: "ArrowLeft", cap: <Icon icon={CaretLeftIcon} size={16} label="Left arrow" /> },
  play: { aria: "Space", cap: "Space" },
  next: { aria: "ArrowRight", cap: <Icon icon={CaretRightIcon} size={16} label="Right arrow" /> },
  last: { aria: "End", cap: "End" },
  prevMoment: { aria: "ArrowUp", cap: <Icon icon={CaretUpIcon} size={16} label="Up arrow" /> },
  nextMoment: {
    aria: "ArrowDown",
    cap: <Icon icon={CaretDownIcon} size={16} label="Down arrow" />,
  },
  flip: { aria: "F", cap: "F" },
};

interface ControlProps {
  id: Key;
  label: string;
  icon: PhosphorIcon;
  disabled: boolean;
  onClick: () => void;
  primary?: boolean;
  pressed?: boolean;
  register: (id: Key, el: HTMLButtonElement | null) => void;
  children?: ReactNode;
}

function Control({
  id,
  label,
  icon,
  disabled,
  onClick,
  primary,
  pressed,
  register,
  children,
}: ControlProps) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            ref={(el) => register(id, el as HTMLButtonElement | null)}
            variant={primary ? "primary" : "quiet"}
            size="md"
            aria-label={label}
            aria-keyshortcuts={KEYS[id].aria}
            aria-pressed={pressed}
            disabled={disabled}
            onClick={onClick}
            data-control={id}
            className={cn("w-10 px-0", primary && "w-12")}
          />
        }
      >
        {children ?? <Icon icon={icon} />}
      </TooltipTrigger>
      <TooltipContent>
        {label}
        <Kbd>{KEYS[id].cap}</Kbd>
      </TooltipContent>
    </Tooltip>
  );
}

/** Where focus goes when the focused control becomes disabled, so it is never dropped. */
const FALLBACK: Record<Key, Key> = {
  first: "next",
  prev: "next",
  next: "prev",
  last: "prev",
  prevMoment: "play",
  nextMoment: "play",
  play: "prev",
  flip: "play",
};

/**
 * The review's transport: first, previous, play or pause, next and last, then the previous and
 * next mistake and the board flip. Icon buttons with names, tooltips that show their keys, and
 * `aria-keyshortcuts`. First and previous are disabled at ply 0, next and last at `lastPly`, and
 * the mistake buttons without moments. If the focused button becomes disabled, focus moves to its
 * neighbour instead of falling to the page. Targets are 44px on coarse pointers (`touchTarget`).
 */
export function ReviewControls({
  ply,
  lastPly,
  playing,
  hasMoments,
  onFirst,
  onPrev,
  onPlayToggle,
  onNext,
  onLast,
  onFlip,
  onPrevMoment,
  onNextMoment,
  className,
}: ReviewControlsProps) {
  const reduced = useReducedMotion();
  const buttons = useRef(new Map<Key, HTMLButtonElement>());
  const focused = useRef<Key | null>(null);
  const register = (id: Key, el: HTMLButtonElement | null) => {
    if (el) buttons.current.set(id, el);
    else buttons.current.delete(id);
  };

  const atStart = ply <= 0;
  const atEnd = ply >= lastPly;

  useLayoutEffect(() => {
    const id = focused.current;
    const el = id ? buttons.current.get(id) : undefined;
    if (!id || !el?.disabled) return;
    const active = document.activeElement;
    if (active !== el && active !== document.body && active !== null) return;
    let next: Key = FALLBACK[id];
    // From a moment button with play the fallback, play is never disabled; from first or prev at
    // the start, next may be disabled too in a game with no moves: then play.
    if (buttons.current.get(next)?.disabled) next = "play";
    buttons.current.get(next)?.focus();
  });

  return (
    // biome-ignore lint/a11y/useSemanticElements: a group of controls, not a form fieldset
    <div
      role="group"
      aria-label="Move controls"
      onFocus={(e) => {
        const id = (e.target as HTMLElement).dataset.control as Key | undefined;
        focused.current = id ?? null;
      }}
      className={cn("flex flex-wrap items-center justify-between gap-x-4 gap-y-3", className)}
    >
      <div className="flex items-center gap-1">
        <Control
          id="first"
          label="First move"
          icon={SkipBackIcon}
          disabled={atStart}
          onClick={onFirst}
          register={register}
        />
        <Control
          id="prev"
          label="Previous move"
          icon={CaretLeftIcon}
          disabled={atStart}
          onClick={onPrev}
          register={register}
        />
        <Control
          id="play"
          label={playing ? "Pause" : "Play"}
          icon={playing ? PauseIcon : PlayIcon}
          disabled={false}
          onClick={onPlayToggle}
          primary
          pressed={playing}
          register={register}
        >
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span
              key={playing ? "pause" : "play"}
              className="flex"
              initial={reduced ? false : { scale: 0.5, opacity: 0, rotate: -30 }}
              animate={{ scale: 1, opacity: 1, rotate: 0 }}
              exit={
                reduced ? { opacity: 0, transition: { duration: 0 } } : { scale: 0.5, opacity: 0 }
              }
              transition={spring.land}
            >
              <Icon icon={playing ? PauseIcon : PlayIcon} weight="fill" />
            </motion.span>
          </AnimatePresence>
        </Control>
        <Control
          id="next"
          label="Next move"
          icon={CaretRightIcon}
          disabled={atEnd}
          onClick={onNext}
          register={register}
        />
        <Control
          id="last"
          label="Last move"
          icon={SkipForwardIcon}
          disabled={atEnd}
          onClick={onLast}
          register={register}
        />
      </div>
      <div className="flex items-center gap-1">
        <Control
          id="prevMoment"
          label="Previous mistake"
          icon={CaretUpIcon}
          disabled={!hasMoments}
          onClick={onPrevMoment}
          register={register}
        />
        <span
          aria-hidden="true"
          className={cn(
            "px-0.5 font-sans text-sm font-medium",
            hasMoments ? "text-ink-soft" : "text-ink-faint",
          )}
        >
          Mistakes
        </span>
        <Control
          id="nextMoment"
          label="Next mistake"
          icon={CaretDownIcon}
          disabled={!hasMoments}
          onClick={onNextMoment}
          register={register}
        />
        <span aria-hidden="true" className="mx-1.5 h-6 w-px bg-rule" />
        <Control
          id="flip"
          label="Flip board"
          icon={ArrowsClockwiseIcon}
          disabled={false}
          onClick={onFlip}
          register={register}
        />
      </div>
    </div>
  );
}
