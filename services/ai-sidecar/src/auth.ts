import { createHash, timingSafeEqual } from "node:crypto";

const digest = (s: string) => createHash("sha256").update(s).digest();

/** Constant-time check of `authorization: Bearer <token>`. */
export function bearerOk(header: string | undefined, token: string): boolean {
  if (!header?.startsWith("Bearer ")) return false;
  return timingSafeEqual(digest(header.slice("Bearer ".length)), digest(token));
}
