import type { WebEnv } from "../../env";

type GoogleEnv = Partial<Pick<WebEnv, "GOOGLE_CLIENT_ID" | "GOOGLE_CLIENT_SECRET">>;

/** Google sign-in, configured only when both env vars are set (no secrets in files). */
export function googleProvider(env: GoogleEnv): {
  google?: { clientId: string; clientSecret: string };
} {
  if (!env.GOOGLE_CLIENT_ID || !env.GOOGLE_CLIENT_SECRET) return {};
  return { google: { clientId: env.GOOGLE_CLIENT_ID, clientSecret: env.GOOGLE_CLIENT_SECRET } };
}
