import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { getAuth } from "../auth";

export type AppUser = {
  id: string;
  email: string;
  name: string;
  birthMonth: string | null;
  marketingEmail: boolean;
  publicShareDefault: boolean;
};

export type AppSession = { user: AppUser; session: { id: string; expiresAt: Date } };

type RawUser = {
  id: string;
  email: string;
  name: string;
  birthMonth?: string | null;
  marketingEmail?: boolean | null;
  publicShareDefault?: boolean | null;
};

function toAppUser(raw: RawUser): AppUser {
  return {
    id: raw.id,
    email: raw.email,
    name: raw.name,
    birthMonth: raw.birthMonth ?? null,
    marketingEmail: raw.marketingEmail ?? false,
    publicShareDefault: raw.publicShareDefault ?? false,
  };
}

/** The signed-in user for these request headers, or null. */
export async function getSession(requestHeaders: Headers): Promise<AppSession | null> {
  const result = await getAuth().api.getSession({ headers: requestHeaders });
  if (!result) return null;
  return {
    user: toAppUser(result.user),
    session: { id: result.session.id, expiresAt: new Date(result.session.expiresAt) },
  };
}

/** For server components: redirects to /sign-in without a session and to /welcome/age before the age is known. */
export async function requireUser(): Promise<AppUser> {
  const current = await getSession(await headers());
  if (!current) redirect("/sign-in");
  if (current.user.birthMonth === null) redirect("/welcome/age");
  return current.user;
}

/** For route handlers: the user, or a 401 `unauthenticated` / 403 `age_required` response. */
export async function requireApiUser(req: Request): Promise<AppUser | Response> {
  const current = await getSession(req.headers);
  if (!current) return Response.json({ error: "unauthenticated" }, { status: 401 });
  if (current.user.birthMonth === null) {
    return Response.json({ error: "age_required" }, { status: 403 });
  }
  return current.user;
}
