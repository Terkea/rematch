// The one definition of a "flag is on" (exactly "1"), shared by env.ts and the lab guard.
export function flagOn(value: string | undefined): boolean {
  return value === "1";
}

// The lab is for design exploration only: unreachable in a production build unless opted in.
export function labEnabled(env: { NODE_ENV?: string; LAB_ENABLED?: string }): boolean {
  return env.NODE_ENV !== "production" || flagOn(env.LAB_ENABLED);
}
