import type { BetterAuthOptions } from "better-auth";

/** Better Auth database hooks. Empty until Task 18; the invite guard is composed in auth.ts. */
export const authDbHooks: NonNullable<BetterAuthOptions["databaseHooks"]> = {};
