import { spawnSync } from "node:child_process";
import { expect } from "@playwright/test";
// Types only: the helpers load @ci/db, which only tsx (with the development condition) resolves.
import type { E2eCoach } from "../helpers/seed-coach";
import type { WorkerStep } from "../helpers/simulate-worker";

// Shared by the coach specs (M5b Task 11): the coach seed and the simulated worker, run through
// tsx like the games seed (support/game-pages.ts).

function tsx(script: string, args: string[]): string {
  const run = spawnSync("node_modules/.bin/tsx", ["--conditions", "development", script, ...args], {
    env: process.env,
    encoding: "utf8",
  });
  expect(run.status, run.stderr).toBe(0);
  return run.stdout;
}

function value(out: string, key: string): string {
  const v = new RegExp(`${key}=(.*)`).exec(out)?.[1]?.trim();
  expect(v, out).toBeTruthy();
  return v as string;
}

/** Seeds (or restores) the coach world for the adult e2e user; `report: false` without a report. */
export function seedCoach(opts: { report?: boolean } = {}): E2eCoach {
  const args = opts.report === false ? ["no-report"] : [];
  return JSON.parse(value(tsx("e2e/helpers/seed-coach.ts", args), "COACH")) as E2eCoach;
}

/** Links an already signed-in user to the e2e account. */
export function linkPeer(email: string): void {
  tsx("e2e/helpers/seed-coach.ts", ["link", email]);
}

/** Plays one step of the worker (rows and event, no AI). */
export function simulate(step: WorkerStep): void {
  tsx("e2e/helpers/simulate-worker.ts", [JSON.stringify(step)]);
}

/** The newest report run (the one a rebuild just requested). */
export function latestRun(): string {
  return value(tsx("e2e/helpers/simulate-worker.ts", ["latest-run"]), "RUN");
}

/** The newest unfinished answer (the one a question just created). */
export function latestAnswer(): string {
  return value(tsx("e2e/helpers/simulate-worker.ts", ["latest-answer"]), "MESSAGE");
}

/** Uses `n` of this week's coach answers, as another device would. */
export function spendAllowance(n: number): void {
  tsx("e2e/helpers/seed-coach.ts", ["spend", String(n)]);
}
