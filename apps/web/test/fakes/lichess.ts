import { createHash, randomBytes } from "node:crypto";
import type { IncomingMessage, ServerResponse } from "node:http";

// The Lichess half of the fakes (Task 18 brief, "Fake servers"): a PKCE authorisation server and
// the two account endpoints. Every value is synthetic, so no real player's data is served.

export const FAKE_LICHESS_TOKEN = "lio_test_token";
export const FAKE_LICHESS_ACCOUNT = { id: "fakeuser", username: "FakeUser" } as const;
export const FAKE_LICHESS_EMAIL = "fakeuser@example.invalid";
/** `login_hint=deny` plays a player who refuses consent. */
export const DENY_HINT = "deny";

type Grant = { challenge: string; clientId: string; redirectUri: string };

const grants = new Map<string, Grant>();
let lastToken: URLSearchParams | null = null;

export function resetLichessFake(): void {
  grants.clear();
  lastToken = null;
}

/** The form body of the last `POST /api/token`, or null. */
export function lastTokenRequest(): URLSearchParams | null {
  return lastToken;
}

function json(res: ServerResponse, status: number, body: unknown): void {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

async function readText(req: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks).toString("utf8");
}

const s256 = (verifier: string) => createHash("sha256").update(verifier).digest("base64url");

function authorised(req: IncomingMessage): boolean {
  return req.headers.authorization === `Bearer ${FAKE_LICHESS_TOKEN}`;
}

/** Handles the Lichess routes; returns false for any other path. */
export async function handleLichess(req: IncomingMessage, res: ServerResponse): Promise<boolean> {
  const url = new URL(req.url ?? "/", "http://fakes");

  if (req.method === "GET" && url.pathname === "/oauth") {
    const q = url.searchParams;
    const redirectUri = q.get("redirect_uri");
    const state = q.get("state");
    if (!redirectUri || !state) {
      json(res, 400, { error: "invalid_request" });
      return true;
    }
    const back = new URL(redirectUri);
    back.searchParams.set("state", state);
    if (q.get("login_hint") === DENY_HINT) {
      back.searchParams.set("error", "access_denied");
      back.searchParams.set("error_description", "user cancelled authorization");
    } else {
      const challenge = q.get("code_challenge");
      if (q.get("code_challenge_method") !== "S256" || !challenge) {
        json(res, 400, { error: "invalid_request" });
        return true;
      }
      const code = `liu_${randomBytes(12).toString("hex")}`;
      grants.set(code, { challenge, clientId: q.get("client_id") ?? "", redirectUri });
      back.searchParams.set("code", code);
    }
    res.writeHead(302, { location: back.toString() });
    res.end();
    return true;
  }

  if (req.method === "POST" && url.pathname === "/api/token") {
    const body = new URLSearchParams(await readText(req));
    lastToken = body;
    const code = body.get("code") ?? "";
    const grant = grants.get(code);
    grants.delete(code);
    const verifier = body.get("code_verifier") ?? "";
    if (
      !grant ||
      body.get("grant_type") !== "authorization_code" ||
      body.get("client_id") !== grant.clientId ||
      body.get("redirect_uri") !== grant.redirectUri ||
      s256(verifier) !== grant.challenge
    ) {
      json(res, 400, { error: "invalid_grant" });
      return true;
    }
    json(res, 200, {
      token_type: "Bearer",
      access_token: FAKE_LICHESS_TOKEN,
      expires_in: 31536000,
    });
    return true;
  }

  if (req.method === "GET" && url.pathname === "/api/account") {
    if (!authorised(req)) json(res, 401, { error: "No such token" });
    else json(res, 200, { ...FAKE_LICHESS_ACCOUNT, perfs: {}, createdAt: 1_700_000_000_000 });
    return true;
  }

  if (req.method === "GET" && url.pathname === "/api/account/email") {
    if (!authorised(req)) json(res, 401, { error: "No such token" });
    else json(res, 200, { email: FAKE_LICHESS_EMAIL });
    return true;
  }

  return false;
}
