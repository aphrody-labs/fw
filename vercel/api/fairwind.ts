import { evaluateDecisions, type DecisionsRequest } from "../../src/decisions.ts";
import { buildA2AAgentCard, buildOpenApiSpec, DEFAULT_ROUTER_PROVIDERS } from "../../src/ai_stack.ts";

export async function GET(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const path = url.searchParams.get("route") || url.pathname;
  if (path.endsWith("/openapi.json")) {
    return Response.json(buildOpenApiSpec(url.origin));
  }
  if (path.endsWith("/agent.json")) {
    return Response.json(buildA2AAgentCard(url.origin));
  }
  if (path.endsWith("/models")) {
    return Response.json({
      object: "list",
      data: DEFAULT_ROUTER_PROVIDERS.flatMap((p) =>
        p.models.map((id) => ({ id, object: "model", owned_by: p.id })),
      ),
    });
  }
  return Response.json({
    ok: true,
    product: "fairwind-vercel",
    runtime: "vercel-fluid-bun1.4",
    protocols: ["openai", "openapi-3.1", "gemini-decision-1", "mcp", "acp", "a2a"],
  });
}

export async function POST(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const body = (await request.json().catch(() => ({}))) as Record<string, unknown>;
  if (url.pathname.includes("decisions") || body.questions) {
    return Response.json(await evaluateDecisions(body as unknown as DecisionsRequest));
  }
  return Response.json({
    ok: true,
    product: "fairwind-vercel",
    receivedKeys: Object.keys(body),
  });
}
