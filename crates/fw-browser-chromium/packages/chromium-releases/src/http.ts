/** The one transport every client goes through: JSON over fetch with timeouts, retries and the upstream error shapes. */

export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

export class HttpError extends Error {
  constructor(
    readonly url: string,
    readonly status: number,
    readonly body: string,
  ) {
    super(`${status} ${url}: ${body.slice(0, 200)}`);
    this.name = "HttpError";
  }
}

/** The upstream rejected the request parameters (Chromium Dash answers with a pydantic error list or `{error}`). */
export class ApiError extends Error {
  constructor(
    readonly url: string,
    readonly detail: unknown,
  ) {
    super(`${url}: ${typeof detail === "string" ? detail : JSON.stringify(detail)}`.slice(0, 400));
    this.name = "ApiError";
  }
}

/** Chrome Status prefixes every JSON body with the XSSI guard `)]}'` and a newline. */
export function stripXssi(text: string): string {
  return text.startsWith(")]}'") ? text.slice(4).replace(/^\r?\n/, "") : text;
}

/** Chromium Dash returns validation failures as 200/400 bodies: a JSON string holding a pydantic error list, or `{error}`. */
export function upstreamError(url: string, value: unknown): ApiError | undefined {
  if (typeof value === "string") {
    try {
      const inner = JSON.parse(value);
      if (Array.isArray(inner) && inner.every(e => e && typeof e === "object" && "type" in e && "msg" in e))
        return new ApiError(url, inner.map((e: { loc?: unknown[]; msg: string }) => `${(e.loc ?? []).join(".")}: ${e.msg}`).join("; "));
    } catch {}
  }
  if (value && typeof value === "object" && !Array.isArray(value) && "error" in value && Object.keys(value).length === 1)
    return new ApiError(url, (value as { error: unknown }).error);
  return undefined;
}

export interface GetOptions {
  fetch?: FetchLike;
  timeoutMs?: number;
  retries?: number;
}

/** GET a JSON document. Retries network failures and 5xx with a short backoff; 4xx fail at once. */
export async function getJson<T>(url: string, options: GetOptions = {}): Promise<T> {
  const doFetch = options.fetch ?? fetch;
  const retries = options.retries ?? 2;
  let last: unknown;
  for (let attempt = 0; attempt <= retries; attempt++) {
    if (attempt) await Bun.sleep(300 * 2 ** (attempt - 1));
    try {
      const res = await doFetch(url, { headers: { accept: "application/json" }, signal: AbortSignal.timeout(options.timeoutMs ?? 30_000) });
      const text = await res.text();
      if (res.status >= 500) throw new HttpError(url, res.status, text);
      let value: unknown;
      try {
        value = JSON.parse(stripXssi(text));
      } catch {
        throw new HttpError(url, res.status, text);
      }
      const bad = upstreamError(url, value);
      if (bad) throw bad;
      if (!res.ok) throw new HttpError(url, res.status, text);
      return value as T;
    } catch (e) {
      last = e;
      if (e instanceof ApiError || (e instanceof HttpError && e.status < 500)) throw e;
    }
  }
  throw last;
}

export function query(params: Record<string, string | number | boolean | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v !== undefined) q.set(k, String(v));
  const s = q.toString();
  return s ? `?${s}` : "";
}
