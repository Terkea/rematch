import type { GlossaryEntry } from "../../../../server/profile/glossary";
import { CONFIDENCE, KPI_MIN_GAMES } from "../../../../server/stats/constants";
import { TRAJECTORY_WINDOW } from "../../../../server/trajectory/model";

// "What is this?" entries of the trajectory (F6, ADR 0089): how each number on it is computed,
// with the real windows, minimum and interval.

const PCT = Math.round(CONFIDENCE * 100);

export const TRAJECTORY_GLOSSARY = {
  trajectory: {
    name: "Rating trajectory",
    meaning: "Where your rating has been, its peak, and how your play then compares with now.",
    measured: `Your rating before each rated game of the time control (on chess.com, your rating after the previous game). "Then" is the ${TRAJECTORY_WINDOW} games around your peak, or your first ${TRAJECTORY_WINDOW} games when the peak is recent; "now" is your last ${TRAJECTORY_WINDOW}. With fewer than ${2 * TRAJECTORY_WINDOW} rated games each window is half of them. A change is called only when the ${PCT}% evidence separates the two windows, with at least ${KPI_MIN_GAMES} games on each side.`,
    whyItMatters:
      "A rating moves for many reasons; comparing how you played then and now shows which ones are yours.",
    tip: "Start with the measure that is clearly worse: it is the likeliest reason for the change.",
  },
  rating: {
    name: "Rating",
    meaning: "Your average rating before each game in the window.",
    measured: `The mean of your pre-game ratings over the window's rated games, with an ${PCT}% range from resampling them. The change is called only when the ${PCT}% range of the difference excludes zero.`,
    whyItMatters: "It tells a lasting change from a few lucky or unlucky games.",
    tip: "Look at what else changed between the two windows before blaming form.",
  },
  // EXP-0015 (ADR 0116): the section and the rating row while the rating's verdict is withheld.
  trajectoryNotCalled: {
    name: "Rating trajectory",
    meaning: "Where your rating has been, its peak, and how your play then compares with now.",
    measured: `Your rating before each rated game of the time control (on chess.com, your rating after the previous game). "Then" is the ${TRAJECTORY_WINDOW} games around your peak, or your first ${TRAJECTORY_WINDOW} games when the peak is recent; "now" is your last ${TRAJECTORY_WINDOW}. With fewer than ${2 * TRAJECTORY_WINDOW} rated games each window is half of them. A change in accuracy, blunders, score or a weakness is called only when the ${PCT}% evidence separates the two windows, with at least ${KPI_MIN_GAMES} games on each side. The rating's own change is shown and not called: see Rating.`,
    whyItMatters:
      "A rating moves for many reasons; comparing how you played then and now shows which ones are yours.",
    tip: "Start with the measure that is clearly worse: it is the likeliest place to work on.",
  },
  ratingNotCalled: {
    name: "Rating",
    meaning: "Your average rating before each game in the window.",
    measured: `The mean of your pre-game ratings over the window's rated games, and the difference between the two windows. The difference is not called better or worse. A rating carries over from one game to the next, so ${TRAJECTORY_WINDOW} ratings in a row hold far less evidence than ${TRAJECTORY_WINDOW} separate results, and in a test on 700 simulated players whose strength never changed, the rule we used told 44% to 78% of them their rating was falling.`,
    whyItMatters:
      "A number that moves with every game is easy to over-read, so the table gives it as two averages and their difference.",
    tip: "Look at the measures that are clearly worse: they are the likeliest places to work on.",
  },
  weakness: {
    name: "Weakness rate",
    meaning: "How often the mistake happened when you had the chance to make it.",
    measured: `Events per chance in each window's games, under the same prior as your weakness estimate, with an ${PCT}% range. It is called better or worse only when one window is at least 80% likely to be lower and the rates differ by at least 15%, with ${KPI_MIN_GAMES} games with the chance on each side.`,
    whyItMatters: "A weakness that grew since your peak is a concrete thing to train.",
    tip: "Open the weakness to see your positions where it happened.",
  },
} satisfies Record<string, GlossaryEntry>;
