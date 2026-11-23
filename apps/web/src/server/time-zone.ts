import { cookies } from "next/headers";
import { TIME_ZONE_COOKIE, timeZoneOf } from "../lib/time-zone";

/** The request's time zone for a page (the browser's cookie), else UTC. */
export async function requestTimeZone(): Promise<string> {
  return timeZoneOf((await cookies()).get(TIME_ZONE_COOKIE)?.value);
}
