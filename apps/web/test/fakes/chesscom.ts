import type { IncomingMessage, ServerResponse } from "node:http";

// The chess.com half of the e2e fakes (Task 18 brief, "Fake servers"). Profiles live in memory;
// `POST /chesscom/__set-location` lets a test write a location, as a player would on chess.com.
// Every value is synthetic, so no real player's data is served.

type FakeProfile = { username: string; displayName: string; location: string | null };

const seed = (): Map<string, FakeProfile> =>
  new Map([
    ["fakeplayer", { username: "fakeplayer", displayName: "FakePlayer", location: "Leeds" }],
    ["fakeplayer2", { username: "fakeplayer2", displayName: "FakePlayer2", location: null }],
  ]);

let profiles = seed();

export function resetChesscomFake(): void {
  profiles = seed();
}

function json(res: ServerResponse, status: number, body: unknown): void {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

async function readBody(req: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } catch {
    return null;
  }
}

/** Handles `/chesscom/...` requests; returns false for any other path. */
export async function handleChesscom(req: IncomingMessage, res: ServerResponse): Promise<boolean> {
  const url = new URL(req.url ?? "/", "http://fakes");
  if (!url.pathname.startsWith("/chesscom/")) return false;

  if (req.method === "POST" && url.pathname === "/chesscom/__set-location") {
    const body = (await readBody(req)) as { username?: unknown; location?: unknown } | null;
    const username = typeof body?.username === "string" ? body.username.toLowerCase() : "";
    const profile = profiles.get(username);
    if (!profile) {
      json(res, 404, { message: "unknown username" });
      return true;
    }
    profile.location = typeof body?.location === "string" ? body.location : null;
    json(res, 200, { ok: true });
    return true;
  }

  if (req.method === "POST" && url.pathname === "/chesscom/__reset") {
    resetChesscomFake();
    json(res, 200, { ok: true });
    return true;
  }

  const player = /^\/chesscom\/pub\/player\/([^/]+)$/.exec(url.pathname);
  if (req.method === "GET" && player?.[1]) {
    const profile = profiles.get(decodeURIComponent(player[1]).toLowerCase());
    if (!profile) {
      json(res, 404, { code: 0, message: "User not found." });
      return true;
    }
    json(res, 200, {
      "@id": `https://api.chess.com/pub/player/${profile.username}`,
      url: `https://www.chess.com/member/${profile.displayName}`,
      username: profile.username,
      player_id: 1,
      status: "basic",
      ...(profile.location === null ? {} : { location: profile.location }),
    });
    return true;
  }

  json(res, 404, { message: "not found" });
  return true;
}
