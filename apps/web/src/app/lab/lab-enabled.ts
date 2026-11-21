// The lab is for design exploration only: unreachable in a production build unless opted in.
export function labEnabled(env: { NODE_ENV?: string; LAB_ENABLED?: string }): boolean {
  return env.NODE_ENV !== "production" || env.LAB_ENABLED === "1";
}
