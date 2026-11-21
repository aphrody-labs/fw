// Stand-in for a service-mode script in `wrangler dev`: answers with who it is and the request it received, so the test can prove
// which script the edge Worker handed a request to without the real sibling code.
export default {
  fetch(request: Request, env: { NAME: string }): Response {
    const url = new URL(request.url);
    return Response.json({ stub: env.NAME, host: url.hostname, path: url.pathname, method: request.method });
  },
} satisfies ExportedHandler<{ NAME: string }>;
