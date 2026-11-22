"use client";

import { type BoardSound, soundForSan, useBoardSounds } from "@ci/ui/audio";
import { Board, EvalBar } from "@ci/ui/board";
import { EvalGraph } from "@ci/ui/eval-graph";
import {
  ArrowCounterClockwiseIcon,
  Icon,
  ListIcon,
  SpeakerHighIcon,
  SpeakerSlashIcon,
} from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { useReducedMotion } from "@ci/ui/motion";
import { Drawer } from "@ci/ui/overlays";
import {
  buttonVariants,
  Kbd,
  Switch,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  touchTarget,
} from "@ci/ui/primitives";
import { ProgressPlaceholder } from "@ci/ui/progress";
import {
  ClockPair,
  type ClockSide,
  moverRow,
  ReviewControls,
  Scoresheet,
  useReviewKeys,
} from "@ci/ui/review";
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { annotationsFor } from "./annotations";
import {
  checkSquare,
  clockAt,
  evalText,
  fenAt,
  graphPoints,
  hasTenths,
  lastMoveAt,
  scoreAt,
  sheetMoves,
  spentAt,
} from "./board-state";
import {
  BASIS_NOTE,
  engineAlternative,
  type LedgerRow,
  ledgerRows,
  moveCommentary,
  moveText,
} from "./move-commentary";
import { endPly, useReveal } from "./reveal/use-reveal";
import {
  initialState,
  PLAY_INTERVAL_MS,
  type ReviewAction,
  type ReviewState,
  reviewReducer,
} from "./review-state";
import type { ReviewView } from "./view-model";

// The single-game review (M5a T10): T7's variant b, "the graph as the spine", with c's "Where it
// went wrong" ledger (ADR 0054, critique-t7.md). The win-chance graph runs across the top and
// scrubs the board; under it the board, and beside it the loudest line on the page: the verdict
// on the shown move. On a phone the graph is a 70px strip, the controls ride in a sticky bar under
// the thumb, and the scoresheet is a drawer.

export type StepKind = "step" | "play" | "jump";

export interface GameReviewViewProps {
  view: ReviewView;
  /** Keys the reveal's "seen on this device" record (R10). Without it no reveal runs. */
  gameId?: string;
  /** The `?ply=` search param, as given. */
  initialPly?: string;
  onStep?: (kind: StepKind, ply: number) => void;
}

/** The URL mirrors the shown ply this long after the last step. */
const URL_DEBOUNCE_MS = 150;
/** A graph scrub reports one jump this long after it stops. */
const SCRUB_SETTLE_MS = 150;
/** While a key is held, step sounds are at most this far apart. */
const REPEAT_SOUND_MS = 120;
/** More ledger rows than this collapse to the three worst. */
const LEDGER_MAX = 8;
const LEDGER_COLLAPSED = 3;
/** The step announcement waits this long after the last step (T5's Scoresheet rule). */
const ANNOUNCE_MS = 300;
/** Said once when the first reveal on the page ends (M8). */
const READY = "Analysis ready";
const SEVERITY_RANK = { blunder: 3, mistake: 2, inaccuracy: 1 } as const;
/** The phone's graph strip, in px (critic: about 70px so the verdict clears the fold). */
const STRIP_HEIGHT = 70;

const WIDE = "(min-width: 768px)";

/** True from 768px. False on the server and the first paint, so the phone layout renders first. */
function useWide(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const mq = window.matchMedia(WIDE);
      mq.addEventListener("change", onChange);
      return () => mq.removeEventListener("change", onChange);
    },
    () => window.matchMedia(WIDE).matches,
    () => false,
  );
}

const WAITING: Record<Exclude<ReviewView["status"], "analysed">, string> = {
  waiting: "This game is waiting for analysis. You can replay the moves now.",
  failed: "Analysis failed for this game.",
  skipped: "This game was skipped: it is a variant or too short to analyse.",
};

const NUMERALS = "[font-variant-numeric:lining-nums_tabular-nums]";

/** The glyph's colour by severity: only the player's ?? in full pen (critic, T7 change 1). */
function severityClass(judgement: string, mine: boolean): string {
  if (!mine) return "text-ink-soft";
  if (judgement === "blunder") return "text-pen-text";
  if (judgement === "mistake") return "text-pen-text/75";
  return "text-ink-soft";
}

function useReview(
  view: ReviewView,
  initialPly: string | undefined,
  onStep: GameReviewViewProps["onStep"],
  playSound: (s: BoardSound) => void,
) {
  const [state, rawDispatch] = useReducer(
    (s: ReviewState, a: ReviewAction) => reviewReducer(view, s, a),
    undefined,
    () => initialState(view, initialPly),
  );
  const stateRef = useRef(state);
  stateRef.current = state;
  const onStepRef = useRef(onStep);
  onStepRef.current = onStep;
  const playSoundRef = useRef(playSound);
  playSoundRef.current = playSound;

  /** The ply a graph scrub started from (updated by every reported step too). */
  const scrubFrom = useRef(state.ply);
  const held = useRef(false);
  const lastSound = useRef(Number.NEGATIVE_INFINITY);
  useEffect(() => {
    // Capture phase, so it runs before the review's own key handler on `window`.
    const down = (e: KeyboardEvent) => {
      held.current = e.repeat;
    };
    const up = () => {
      held.current = false;
    };
    window.addEventListener("keydown", down, true);
    window.addEventListener("keyup", up, true);
    return () => {
      window.removeEventListener("keydown", down, true);
      window.removeEventListener("keyup", up, true);
    };
  }, []);

  /** One sound for a move the user stepped or jumped to; a step back sounds the move it undoes. */
  const sound = useCallback(
    (from: number, to: number) => {
      if (from === to) return;
      // A held key plays its first step, then at most one sound per REPEAT_SOUND_MS.
      const t = performance.now();
      if (held.current && t - lastSound.current < REPEAT_SOUND_MS) return;
      lastSound.current = t;
      const ply = to > from ? to : from;
      const san = view.plies[ply - 1]?.san;
      if (san) playSoundRef.current(soundForSan(san));
    },
    [view],
  );

  /** Dispatch, then report the ply it lands on (when it moved). */
  const act = useCallback(
    (a: ReviewAction, kind: StepKind | null) => {
      const prev = stateRef.current;
      const next = reviewReducer(view, prev, a);
      rawDispatch(a);
      stateRef.current = next;
      if (kind) {
        onStepRef.current?.(kind, next.ply);
        sound(prev.ply, next.ply);
        scrubFrom.current = next.ply;
      }
    },
    [view, sound],
  );

  // Autoplay: one ply a second, stopping at the end.
  useEffect(() => {
    if (!state.playing) return;
    const id = setInterval(() => act({ type: "tick" }, "play"), PLAY_INTERVAL_MS);
    return () => clearInterval(id);
  }, [state.playing, act]);

  // Mirror the ply to ?ply= (never on the first render).
  const mounted = useRef(false);
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    const t = setTimeout(() => {
      const url = new URL(window.location.href);
      url.searchParams.set("ply", String(state.ply));
      window.history.replaceState(window.history.state, "", url.toString());
    }, URL_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [state.ply]);

  // The graph selects on every scrub frame; report the jump once it settles.
  const scrub = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (scrub.current) clearTimeout(scrub.current);
    },
    [],
  );
  const fromGraph = useCallback(
    (ply: number) => {
      act({ type: "go", ply }, null);
      if (scrub.current) clearTimeout(scrub.current);
      scrub.current = setTimeout(() => {
        onStepRef.current?.("jump", stateRef.current.ply);
        // A scrub sounds once, when it settles, never per frame.
        sound(scrubFrom.current, stateRef.current.ply);
        scrubFrom.current = stateRef.current.ply;
      }, SCRUB_SETTLE_MS);
    },
    [act, sound],
  );

  const controls = {
    first: () => act({ type: "first" }, "jump"),
    prev: () => act({ type: "step", by: -1 }, "step"),
    next: () => act({ type: "step", by: 1 }, "step"),
    last: () => act({ type: "last" }, "jump"),
    prevMoment: () => act({ type: "moment", dir: -1 }, "jump"),
    nextMoment: () => act({ type: "moment", dir: 1 }, "jump"),
    play: () => act({ type: "togglePlay" }, null),
    flip: () => act({ type: "flip" }, null),
    engine: () => act({ type: "toggleEngine" }, null),
    go: (ply: number) => act({ type: "go", ply }, "jump"),
    /** Not a step by the user: nothing is reported (a replay setting the end state). */
    goQuiet: (ply: number) => act({ type: "go", ply }, null),
    fromGraph,
  };
  useReviewKeys(controls);
  return { state, controls };
}

/** The spent line, placed under the row of the player who made the shown move (T13). */
function spentFor(spent: { seconds: number } | null, ply: number, flipped: boolean) {
  return spent ? { ...spent, by: moverRow(ply, flipped ? "white" : "black") } : null;
}

export function GameReviewView({ view, gameId, initialPly, onStep }: GameReviewViewProps) {
  const sounds = useBoardSounds();
  const { state, controls: c } = useReview(view, initialPly, onStep, sounds.play);
  const reducedMotion = useReducedMotion();
  // The analysis reveal (T11): while it runs it decides the shown ply and marks; the review's own
  // state already sits on the end state (the worst moment), so when it ends nothing jumps.
  const reveal = useReveal(view, gameId, {
    reducedMotion,
    explicitPly: initialPly !== undefined,
  });
  const revealing = reveal.phase === "running";
  const { playing, orientation, showEngine } = state;
  const ply = reveal.state.ply ?? state.ply;
  const wide = useWide();
  const engineId = useId();
  const lastPly = view.plies.length;
  const analysed = view.status === "analysed";

  const sheet = useMemo(() => sheetMoves(view), [view]);
  const points = useMemo(() => graphPoints(view), [view]);
  const ledger = useMemo(() => ledgerRows(view), [view]);
  const tenths = useMemo(() => hasTenths(view), [view]);
  const fen = fenAt(view, ply);
  const check = useMemo(() => checkSquare(fen), [fen]);
  const ownAnnotations = useMemo(
    () => annotationsFor(view, ply, showEngine),
    [view, ply, showEngine],
  );
  const annotations = reveal.state.annotations ?? ownAnnotations;
  const lastMove = lastMoveAt(view, ply);
  const score = scoreAt(view, ply);
  const ev = evalText(score);
  const move = view.plies[ply - 1];
  const text = moveText(view, ply);
  const verdict = moveCommentary(view, ply);
  const alternative = engineAlternative(view, ply);
  // The announcer follows the review's ply, not the reveal's frames: nothing is read mid-reveal.
  const ownPly = state.ply;
  const spokenOwn = moveCommentary(view, ownPly) ?? moveText(view, ownPly) ?? "Start position";

  // One page-level announcer at every width (fix round 1): the scoresheet's own is in a drawer on
  // phones. Polite, 300ms after the last step, silent while autoplaying and on first paint; the
  // move play stops on is read when it stops.
  const [announcement, setAnnouncement] = useState("");
  // It reads a text only when it differs from the last one read (or from the first paint's), so
  // an effect re-run (StrictMode, the end of a reveal) never re-reads the shown move.
  const lastSpoken = useRef(spokenOwn);
  useEffect(() => {
    if (playing || revealing || spokenOwn === lastSpoken.current) return;
    const t = setTimeout(() => {
      lastSpoken.current = spokenOwn;
      setAnnouncement(spokenOwn);
    }, ANNOUNCE_MS);
    return () => clearTimeout(t);
  }, [spokenOwn, playing, revealing]);

  // M8: when the first reveal on this page ends, say so once (polite, in the same announcer).
  const readyAnnounced = useRef(false);
  const ranRevealRef = useRef(false);
  if (revealing && reveal.mode !== null) ranRevealRef.current = true;
  useEffect(() => {
    if (revealing || !ranRevealRef.current || readyAnnounced.current) return;
    readyAnnounced.current = true;
    setAnnouncement(READY);
  }, [revealing]);

  // Skip and Replay swap in one place; when the one with focus goes, focus moves to the other.
  const revealControlFocused = useRef(false);
  const revealControl = useRef<HTMLButtonElement | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: refocus on each swap
  useEffect(() => {
    if (revealControlFocused.current) revealControl.current?.focus();
  }, [revealing]);
  const replay = () => {
    // The replay ends where every reveal ends: the worst moment, with the review state on it.
    c.goQuiet(endPly(view));
    reveal.replay();
  };
  const showSkip = revealing && reveal.mode === "full";
  const showReplay = !revealing && analysed && gameId !== undefined && !reducedMotion;
  // The control's slots are in the server HTML whenever a reveal could ever run here, so the
  // button arriving or swapping never moves the board (review I1). From 768px the slot is a
  // header row above the board (M8); on a phone it sits at the end of the move line, which is
  // already there, so the board and the verdict keep their first-screen places.
  const slots = analysed && gameId !== undefined;
  const control = showSkip ? (
    <button
      key="skip"
      ref={revealControl}
      type="button"
      onClick={reveal.skip}
      onFocus={() => {
        revealControlFocused.current = true;
      }}
      onBlur={() => {
        revealControlFocused.current = false;
      }}
      className={buttonVariants({ variant: "quiet", size: "sm" })}
    >
      Skip
    </button>
  ) : showReplay ? (
    <button
      key="replay"
      ref={revealControl}
      type="button"
      onClick={replay}
      onFocus={() => {
        revealControlFocused.current = true;
      }}
      onBlur={() => {
        revealControlFocused.current = false;
      }}
      className={buttonVariants({ variant: "quiet", size: "sm" })}
    >
      <Icon icon={ArrowCounterClockwiseIcon} size={16} />
      Replay analysis
    </button>
  ) : null;
  // During a beat the green arrow is named only once the beat's marks are on the board (I2).
  const marksDrawn = !revealing || (reveal.state.annotations?.length ?? 0) > 0;
  const flipped = orientation === "black";
  const whiteToMove = fen.split(" ")[1] === "w";

  const side = (color: "white" | "black"): ClockSide => ({
    name: view[color].name,
    rating: view[color].rating,
    seconds: view.hasClocks ? clockAt(view, color, ply) : null,
    toMove: (color === "white") === whiteToMove && ply < lastPly,
  });

  const transport = {
    ply,
    lastPly,
    playing,
    hasMoments: view.moments.length > 0,
    onFirst: c.first,
    onPrev: c.prev,
    onPlayToggle: c.play,
    onNext: c.next,
    onLast: c.last,
    onFlip: c.flip,
    onPrevMoment: c.prevMoment,
    onNextMoment: c.nextMoment,
  };

  const moveLabel = move ? `${text ?? move.san}` : "the start";
  const scoresheet = (className: string) => (
    <Scoresheet
      moves={sheet}
      currentPly={ply}
      onSelectPly={c.go}
      label={`Moves, ${view.title}`}
      announce={false}
      className={className}
    />
  );

  return (
    <div
      data-review
      data-ply={ply}
      data-fen={fen}
      data-reveal={reveal.phase}
      data-reveal-mode={reveal.mode ?? undefined}
      data-animate-annotations={String(reveal.state.animateAnnotations)}
      className="flex flex-col gap-5 md:gap-6"
    >
      <p
        data-testid="step-status"
        role="status"
        aria-live="polite"
        aria-atomic="true"
        className="sr-only"
      >
        {announcement}
      </p>
      {/* The spine: the whole game at once, and the scrubber for the board under it. */}
      {view.status === "failed" || view.status === "skipped" ? null : (
        <section
          aria-label="Win chance"
          className="flex flex-col gap-2 rounded-sheet bg-sheet px-(--space-panel-pad) py-3 max-md:-mx-(--space-gutter) max-md:rounded-none max-md:px-(--space-gutter) max-md:py-2 md:min-h-54 md:py-4"
        >
          {analysed ? (
            <>
              <p className="flex items-baseline justify-between gap-4 text-xs text-ink-faint max-md:sr-only">
                <span>White&rsquo;s win chance, White at the top</span>
                <span>Drag or use the arrow keys to step</span>
              </p>
              <EvalGraph
                key={reveal.run}
                drawIn={reveal.state.drawGraph}
                points={points}
                label={`Win chance, ${view.title}`}
                currentPly={ply}
                onSelectPly={c.fromGraph}
                phases={view.phases}
                playerColor={view.playerColor}
                {...(wide ? {} : { compact: true, height: STRIP_HEIGHT })}
              />
            </>
          ) : (
            <ProgressPlaceholder shape="graph" verb="Analysing" />
          )}
        </section>
      )}

      {analysed ? null : (
        <p className="max-w-prose font-serif text-prose text-ink" data-testid="waiting">
          {WAITING[view.status as Exclude<ReviewView["status"], "analysed">]}
        </p>
      )}

      <div className="grid gap-x-(--space-gutter) gap-y-5 lg:grid-cols-[auto_minmax(0,1fr)]">
        <div className="flex min-w-0 flex-col gap-3">
          {slots ? (
            // M8: the board column's header row, right-aligned, never over the board. A distinct
            // key per button, so a press that skips can never land as a click on Replay.
            <div data-reveal-controls="board" className="-mb-1 flex h-8 justify-end max-md:hidden">
              {wide ? control : null}
            </div>
          ) : null}
          <div className="flex w-fit flex-col gap-2 md:flex-row md:gap-3">
            {score ? (
              <EvalBar
                score={score.score}
                fen={fen}
                orientation={orientation}
                label={`Evaluation, ${ev}`}
                ends
                className="md:my-2"
              />
            ) : null}
            <Board
              fen={fen}
              label={`${view.title}, after ${moveLabel}, seen from ${flipped ? "Black" : "White"}`}
              orientation={orientation}
              {...(lastMove ? { lastMove } : {})}
              {...(check ? { check } : {})}
              annotations={annotations}
              animateAnnotations={reveal.state.animateAnnotations}
            />
          </div>
          <ReviewControls {...transport} className={cn("max-md:hidden", score && "md:pl-7")} />
        </div>

        <div className="flex min-w-0 flex-col gap-5">
          {/* The verdict on the shown move: the loudest line on the page. */}
          <section aria-label="This move" className="flex flex-col gap-2">
            <div
              className={cn("flex items-baseline gap-3 font-sans text-sm text-ink-soft", NUMERALS)}
            >
              <span className="font-medium text-ink">{text ?? "Start position"}</span>
              {ev ? <span data-testid="eval-readout">{ev}</span> : null}
              {slots ? (
                // The phone's slot: 32px tall but pulled into the line's own height.
                <span
                  data-reveal-controls="phone"
                  className="-my-1.5 ml-auto flex h-8 self-center md:hidden"
                >
                  {wide ? null : control}
                </span>
              ) : null}
              <SoundToggle
                muted={sounds.muted}
                onChange={(muted) => sounds.setMuted(muted)}
                className={slots ? undefined : "ml-auto"}
              />
            </div>
            <p
              data-testid="verdict"
              className="min-h-21 max-w-prose font-serif-italic text-xl text-ink italic md:min-h-22.5 md:text-2xl"
            >
              {verdict ?? (move ? "" : "Step through the game, or jump to a mistake.")}
            </p>
            {/* One line reserved, so the column under it doesn't jump as moves change (I2). */}
            <div data-testid="arrow-key-slot" className="min-h-5 text-sm">
              {alternative && marksDrawn ? (
                <p data-testid="arrow-key" className="text-ink-soft">
                  {`Green arrow: ${alternative}, the engine's choice instead.`}
                </p>
              ) : null}
            </div>
          </section>
          {analysed ? (
            <div className="flex items-center gap-3 text-sm text-ink-soft">
              <Switch checked={showEngine} onCheckedChange={c.engine} aria-labelledby={engineId} />
              <span id={engineId}>{"Show the engine's next move"}</span>
              <Kbd className="max-md:hidden">B</Kbd>
            </div>
          ) : null}

          <ClockPair
            top={side(flipped ? "white" : "black")}
            bottom={side(flipped ? "black" : "white")}
            spent={spentFor(view.hasClocks ? spentAt(view, ply) : null, ply, flipped)}
            tenths={tenths}
          />

          {analysed ? <Ledger rows={ledger} ply={ply} onGo={c.go} /> : null}

          {/* The drawer stays mounted (its trigger hides from 768px); one scoresheet in the DOM. */}
          <Drawer.Root>
            <Drawer.Trigger
              className={cn(buttonVariants({ variant: "outline" }), "self-start md:hidden")}
            >
              <Icon icon={ListIcon} size={20} />
              All moves
            </Drawer.Trigger>
            <Drawer.Content aria-describedby={undefined}>
              <Drawer.Title>Moves</Drawer.Title>
              {wide ? null : scoresheet("h-96")}
            </Drawer.Content>
          </Drawer.Root>
          {wide ? scoresheet("h-96 lg:h-80") : null}
        </div>
      </div>

      {/* Under the thumb on a phone: previous and next mistake, previous, play and next. */}
      <div className="sticky bottom-0 z-10 -mx-(--space-gutter) border-t border-rule bg-paper px-(--space-gutter) py-2 md:hidden">
        <ReviewControls {...transport} layout="thumb" />
      </div>
    </div>
  );
}

/**
 * Board sounds as an icon switch at the end of the verdict line: it adds no row, so the layout
 * is the same on a phone. A coarse pointer gets a 44px target; the tooltip names it.
 */
function SoundToggle({
  muted,
  onChange,
  className,
}: {
  muted: boolean;
  onChange: (muted: boolean) => void;
  className?: string | undefined;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            role="switch"
            aria-checked={!muted}
            aria-label="Board sounds"
            data-testid="sound-toggle"
            onClick={() => onChange(!muted)}
            className={cn(
              "relative -my-3 inline-flex size-8 shrink-0 self-center cursor-pointer items-center justify-center rounded-field text-ink-soft",
              "transition-colors duration-(--dur-quick) ease-out hover:text-ink",
              "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
              touchTarget,
              className,
            )}
          />
        }
      >
        <Icon icon={muted ? SpeakerSlashIcon : SpeakerHighIcon} size={20} />
      </TooltipTrigger>
      <TooltipContent>{muted ? "Board sounds are off" : "Board sounds are on"}</TooltipContent>
    </Tooltip>
  );
}

function Ledger({
  rows,
  ply,
  onGo,
}: {
  rows: LedgerRow[];
  ply: number;
  onGo: (ply: number) => void;
}) {
  const [all, setAll] = useState(false);
  const collapsed = !all && rows.length > LEDGER_MAX;
  const shown = useMemo(() => {
    if (!collapsed) return rows;
    // The three worst (?? before ?, before ?!, then by drop), plus the shown move when it is one
    // of the player's mistakes; listed in game order.
    const worst = [...rows]
      .sort(
        (x, y) =>
          SEVERITY_RANK[y.judgement] - SEVERITY_RANK[x.judgement] ||
          y.before - y.after - (x.before - x.after) ||
          x.ply - y.ply,
      )
      .slice(0, LEDGER_COLLAPSED)
      .map((r) => r.ply);
    const keep = new Set([...worst, ply]);
    return rows.filter((r) => keep.has(r.ply));
  }, [collapsed, rows, ply]);

  if (rows.length === 0)
    return (
      <section className="flex flex-col gap-1">
        <h2 className="font-serif text-xl font-semibold text-ink">Where it went wrong</h2>
        <p className="text-sm text-ink-soft">The engine found nothing of yours to mark.</p>
      </section>
    );

  return (
    <section className="flex flex-col gap-2">
      <div className="flex flex-col gap-x-4 sm:flex-row sm:items-baseline sm:justify-between">
        <h2 className="font-serif text-xl font-semibold text-ink">Where it went wrong</h2>
        <span className="text-xs text-ink-faint">Your win chance, before to after</span>
      </div>
      <ol aria-label="Where it went wrong" className="flex flex-col border-t border-rule">
        {shown.map((r) => {
          const note = BASIS_NOTE[r.basis];
          return (
            <li key={r.ply} className="border-b border-rule">
              <button
                type="button"
                data-ply={r.ply}
                onClick={() => onGo(r.ply)}
                aria-current={r.ply === ply ? "step" : undefined}
                className={cn(
                  "grid min-h-11 w-full grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-4 px-2 py-2 text-left",
                  "hover:bg-well aria-[current=step]:bg-well",
                  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ink",
                )}
              >
                <span className={cn("font-serif text-lg font-semibold text-ink", NUMERALS)}>
                  {r.text.slice(0, r.text.length - glyphLength(r.judgement))}
                  <span
                    data-severity={r.judgement}
                    className={cn("tracking-glyph", severityClass(r.judgement, true))}
                  >
                    {r.text.slice(r.text.length - glyphLength(r.judgement))}
                  </span>
                </span>
                <span className={cn("text-right font-sans text-base text-ink", NUMERALS)}>
                  {r.before}% to {r.after}%
                </span>
                {note ? (
                  <span className="col-span-2 font-serif-italic text-sm text-ink-soft italic">
                    {note}
                  </span>
                ) : null}
              </button>
            </li>
          );
        })}
      </ol>
      {collapsed ? (
        <button
          type="button"
          onClick={() => setAll(true)}
          className="min-h-11 self-start text-sm font-medium text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink"
        >
          {/* Says what it lists (T13 critic): every move the ledger marked, not only the worst. */}
          Show all {rows.length} marked moves
        </button>
      ) : null}
    </section>
  );
}

const glyphLength = (j: LedgerRow["judgement"]) => (j === "mistake" ? 1 : 2);
