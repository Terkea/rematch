import type { GenericOAuthConfig } from "better-auth/plugins/generic-oauth";
import type { WebEnv } from "../../env";
import { chesscomOAuthProvider } from "./chesscom-oauth";
import { lichessProvider } from "./lichess";

/** Generic OAuth providers; each returns null while it is not configured. */
export function oauthProviders(env: WebEnv): GenericOAuthConfig[] {
  return [lichessProvider(env), chesscomOAuthProvider(env)].filter(
    (provider): provider is GenericOAuthConfig => Boolean(provider),
  );
}
