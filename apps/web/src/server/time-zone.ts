import { cookies } from "next/headers";
import { TIME_ZONE_COOKIE } from "../lib/time-zone";
import { timeZoneOf } from "./train/model";

/** The request's time zone for a page (the browser's cookie), else UTC. */
export async function requestTimeZone(): Promise<string> {
  return timeZoneOf((await cookies()).get(TIME_ZONE_COOKIE)?.value);
}
