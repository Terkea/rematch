import { handleAuthRequest } from "../../../../server/auth";

export const dynamic = "force-dynamic";

export function GET(req: Request): Promise<Response> {
  return handleAuthRequest(req);
}

export function POST(req: Request): Promise<Response> {
  return handleAuthRequest(req);
}
