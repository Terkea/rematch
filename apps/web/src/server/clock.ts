import { cookies } from "next/headers";
import { getEnv } from "../env";

/**
 * The cookie the visual e2e sets to pin "now" for one browser context (ADR 0109). The server
 * reads it only with `E2E_CLOCK=1`, which only the e2e compose overlay sets.
 */
export const E2E_NOW_COOKIE = "ci-e2e-now";

/**
 * The request's current time: the wall clock, or under `E2E_CLOCK=1` the time in the
 * `ci-e2e-now` cookie, so the visual baselines of pages that run to today (the weakness,
 * area and progress timelines) hold on every day.
 */
export async function requestNow(): Promise<Date> {
  if (getEnv().E2E_CLOCK) {
    const raw = (await cookies()).get(E2E_NOW_COOKIE)?.value;
    const pinned = raw === undefined ? Number.NaN : Date.parse(raw);
    if (!Number.isNaN(pinned)) return new Date(pinned);
  }
  return new Date();
}
