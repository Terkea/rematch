import type { GenericOAuthConfig } from "better-auth/plugins/generic-oauth";
import type { WebEnv } from "../../env";

/** Lichess OAuth provider. Stub until Task 18 (deferred by ADR 0040). */
export function lichessProvider(_env: WebEnv): GenericOAuthConfig | null {
  return null;
}
