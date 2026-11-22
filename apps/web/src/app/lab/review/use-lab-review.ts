"use client";

import type { Square } from "@ci/ui/board";
import { type ClockSide, useReviewKeys } from "@ci/ui/review";
import { Chess } from "chess.js";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  annotationsAt,
  clockAt,
  commentaryAt,
  fenAt,
  graphPoints,
  hasTenths,
  lastMoveAt,
  moveText,
  scoreAt,
  sheetMoves,
  spentAt,
} from "./model";
import type { ReviewView } from "./view";

// The review state shared by the three lab variants (lab only; Task 10 owns the product reducer).

export const PLAY_MS = 1000;

function checkSquare(fen: string): Square | undefined {
  const chess = new Chess(fen);
  if (!chess.inCheck()) return undefined;
  for (const row of chess.board())
    for (const cell of row)
      if (cell?.type === "k" && cell.color === chess.turn()) return cell.square as Square;
  return undefined;
}

export function useLabReview(game: ReviewView) {
  const lastPly = game.plies.length;
  const moments = useMemo(() => [...game.moments].sort((a, b) => a - b), [game.moments]);
  const [ply, setPly] = useState(() => game.moments[0] ?? lastPly);
  const [playing, setPlaying] = useState(false);
  const [orientation, setOrientation] = useState<"white" | "black">(game.playerColor);

  const go = useCallback(
    (next: number) => {
      setPlaying(false);
      setPly(Math.min(lastPly, Math.max(0, next)));
    },
    [lastPly],
  );
  const moment = useCallback(
    (dir: 1 | -1) => {
      const target = dir === 1 ? moments.find((m) => m > ply) : moments.findLast((m) => m < ply);
      if (target !== undefined) go(target);
    },
    [moments, ply, go],
  );
  const togglePlay = useCallback(() => {
    setPlaying((p) => {
      if (!p && ply >= lastPly) setPly(0);
      return !p;
    });
  }, [ply, lastPly]);
  const flip = useCallback(() => setOrientation((o) => (o === "white" ? "black" : "white")), []);

  useEffect(() => {
    if (!playing) return;
    const id = setInterval(() => {
      setPly((p) => {
        if (p >= lastPly - 1) setPlaying(false);
        return Math.min(lastPly, p + 1);
      });
    }, PLAY_MS);
    return () => clearInterval(id);
  }, [playing, lastPly]);

  useReviewKeys({
    prev: () => go(ply - 1),
    next: () => go(ply + 1),
    first: () => go(0),
    last: () => go(lastPly),
    prevMoment: () => moment(-1),
    nextMoment: () => moment(1),
    play: togglePlay,
    flip,
  });

  const sheet = useMemo(() => sheetMoves(game), [game]);
  const points = useMemo(() => graphPoints(game), [game]);
  const tenths = useMemo(() => hasTenths(game), [game]);
  const fen = fenAt(game, ply);
  const check = useMemo(() => checkSquare(fen), [fen]);
  const annotations = useMemo(() => annotationsAt(game, ply), [game, ply]);
  const lastMove = lastMoveAt(game, ply);
  const whiteToMove = fen.split(" ")[1] === "w";
  const side = (color: "white" | "black"): ClockSide => ({
    name: game[color].name,
    rating: game[color].rating,
    seconds: game.hasClocks ? clockAt(game, color, ply) : null,
    toMove: (color === "white") === whiteToMove && ply < lastPly,
  });
  const flipped = orientation === "black";
  const title = `${game.white.name} v ${game.black.name}`;
  const move = game.plies[ply - 1];

  return {
    game,
    title,
    ply,
    lastPly,
    playing,
    orientation,
    moments,
    sheet,
    points,
    tenths,
    fen,
    check,
    lastMove,
    annotations,
    move,
    moveText: moveText(game, ply),
    score: scoreAt(game, ply),
    commentary: commentaryAt(game, ply),
    clocks: {
      top: side(flipped ? "white" : "black"),
      bottom: side(flipped ? "black" : "white"),
      spent: game.hasClocks ? spentAt(game, ply) : null,
    },
    boardLabel: `${title}, after ${move ? move.san : "the start"}, seen from ${flipped ? "Black" : "White"}`,
    go,
    moment,
    togglePlay,
    flip,
    controls: {
      ply,
      lastPly,
      playing,
      hasMoments: moments.length > 0,
      onFirst: () => go(0),
      onPrev: () => go(ply - 1),
      onPlayToggle: togglePlay,
      onNext: () => go(ply + 1),
      onLast: () => go(lastPly),
      onFlip: flip,
      onPrevMoment: () => moment(-1),
      onNextMoment: () => moment(1),
    },
  };
}

export type LabReview = ReturnType<typeof useLabReview>;
