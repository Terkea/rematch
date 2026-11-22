import { z } from "zod";
import { flagOn } from "./app/lab/lab-enabled";

const flag = z.string().optional().transform(flagOn);
// Default-on switch: off only when set to exactly "0" (ADR-WEB-3, invite-only private beta).
const defaultOn = z
  .string()
  .optional()
  .transform((value) => value !== "0");
const list = z
  .string()
  .default("")
  .transform((value) =>
    value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean),
  );
const emailList = z
  .string()
  .default("")
  .transform((value) =>
    value
      .split(",")
      .map((email) => email.trim().toLowerCase())
      .filter(Boolean),
  );

/**
 * docker-compose.yml's fallback BETTER_AUTH_SECRET. It is published in the repository, so it
 * signs nothing outside local development: a production server refuses it (review-t27 I2).
 */
export const DEV_AUTH_SECRET = "dev-only-secret-do-not-use-outside-local-compose";

const schema = z.object({
  DATABASE_URL: z.string().min(1),
  REDIS_URL: z.string().min(1),
  BETTER_AUTH_SECRET: z.string().min(32),
  BETTER_AUTH_URL: z.string().default("http://localhost:3100"),
  BETTER_AUTH_TRUSTED_ORIGINS: z.string().default(""),
  SMTP_URL: z.string().default("smtp://mailpit:1025"),
  MAIL_FROM: z.string().default("chess-improver <no-reply@chess-improver.local>"),
  GOOGLE_CLIENT_ID: z.string().optional(),
  GOOGLE_CLIENT_SECRET: z.string().optional(),
  LICHESS_CLIENT_ID: z.string().default("chess-improver"),
  LICHESS_AUTHORIZE_BASE_URL: z.string().default("https://lichess.org"),
  LICHESS_API_BASE_URL: z.string().default("https://lichess.org"),
  CHESSCOM_API_BASE_URL: z.string().default("https://api.chess.com"),
  CHESSCOM_ENABLED: flag,
  CHESSCOM_OAUTH_ENABLED: flag,
  // chess.com OAuth (unverified until chess.com approves the application, spec §18.1).
  CHESSCOM_CLIENT_ID: z.string().optional(),
  CHESSCOM_CLIENT_SECRET: z.string().optional(),
  LAB_ENABLED: flag,
  INVITE_ONLY: defaultOn,
  INVITE_ALLOWLIST: emailList,
  // Client IP for rate limits (ADR-WEB-3). Empty: no proxy, so forwarding headers are ignored.
  TRUSTED_PROXIES: list,
  IP_ADDRESS_HEADERS: z
    .string()
    .default("x-forwarded-for")
    .transform((value) =>
      value
        .split(",")
        .map((item) => item.trim().toLowerCase())
        .filter(Boolean),
    ),
});

export type WebEnv = z.infer<typeof schema>;

let cached: WebEnv | undefined;

/** Parse and validate the environment. Only the process environment is cached. */
export function getEnv(source?: Record<string, string | undefined>): WebEnv {
  if (source) return parse(source);
  cached ??= parse(process.env);
  return cached;
}

export function resetEnvCache(): void {
  cached = undefined;
}

function parse(source: Record<string, string | undefined>): WebEnv {
  const result = schema.safeParse(source);
  if (!result.success) {
    const problems = result.error.issues
      .map((issue) => `${issue.path.join(".")}: ${issue.message}`)
      .join("; ");
    throw new Error(`Invalid environment: ${problems}`);
  }
  const secret = result.data.BETTER_AUTH_SECRET;
  if (
    source.NODE_ENV === "production" &&
    (secret === DEV_AUTH_SECRET || secret.startsWith("dev-only-"))
  ) {
    throw new Error(
      "Invalid environment: BETTER_AUTH_SECRET is the public compose default; set a random secret of at least 32 characters",
    );
  }
  return result.data;
}
