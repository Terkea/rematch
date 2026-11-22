import type { GlossaryEntry } from "../../../../server/profile/glossary";
import { CONFIDENCE, KPI_MIN_GAMES } from "../../../../server/stats/constants";

export type { GlossaryEntry };

// "What is this?" entries for the game-level stats of the filter pages (T3): the overview's
// White | Black row and the period comparison. Each says how the number on the page is computed
// (ADR 0083 §6-7), with the real minimum and interval, nothing more.

const PCT = Math.round(CONFIDENCE * 100);

export const STATS_GLOSSARY = {
  accuracy: {
    name: "Accuracy",
    meaning: "How close your moves were to the engine's best moves, as a percentage per game.",
    measured: `The average of your game accuracy (the Lichess formula) over the analysed games the filters select, with an ${PCT}% range from resampling those games. It needs at least ${KPI_MIN_GAMES} analysed games.`,
    whyItMatters: "It sums up how cleanly you played, win or lose.",
    tip: "Open your lowest-accuracy games first: they hold the moves worth reviewing.",
  },
  blunders: {
    name: "Blunders per game",
    meaning: "How many blunders you made in an average game.",
    measured: `Your own moves judged blunders, counted per analysed game and averaged over the games the filters select, with an ${PCT}% range. It needs at least ${KPI_MIN_GAMES} analysed games.`,
    whyItMatters: "One blunder often decides a game at club level.",
    tip: "Before each move, check what your opponent's last move attacks.",
  },
  score: {
    name: "Score",
    meaning: "The points you took per game: a win is 1, a draw a half and a loss 0.",
    measured: `Points over finished games (unfinished ones are left out), with an ${PCT}% Wilson range, and wins, draws and losses written out. It needs at least ${KPI_MIN_GAMES} finished games.`,
    whyItMatters: "It is the result that moves your rating.",
    tip: "Compare your score with your accuracy: a low score with good accuracy points to the clock or the endgame.",
  },
  colour_split: {
    name: "By colour",
    meaning: "The same numbers for your games with White and your games with Black, apart.",
    measured: `Each colour is counted on its own games only, and below ${KPI_MIN_GAMES} games a colour shows a dash and how many more it needs.`,
    whyItMatters:
      "Many players are much stronger with one colour, and an average of both hides it.",
    tip: "If one colour lags, look at your openings with it first.",
  },
  compare: {
    name: "Compare periods",
    meaning: "Your numbers in an earlier period A against a later period B.",
    measured: `A change is called only when both periods have at least ${KPI_MIN_GAMES} games and the ${PCT}% range of the difference (from resampling each period's games) is all above or all below zero. Otherwise it reads "No clear change", or says how many more games a period needs.`,
    whyItMatters:
      "A few good or bad games can look like a trend; this keeps chance from being called progress.",
    tip: "Compare periods of at least a few dozen games each before you change your training.",
  },
} satisfies Record<string, GlossaryEntry>;
