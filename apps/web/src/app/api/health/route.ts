import { checkHealth } from "../../../server/health";

export const dynamic = "force-dynamic";

export async function GET(): Promise<Response> {
  const health = await checkHealth();
  return Response.json(health, { status: health.ok ? 200 : 503 });
}
