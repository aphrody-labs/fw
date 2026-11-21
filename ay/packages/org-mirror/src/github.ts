/** GitHub transport: GraphQL and REST over fetch, with the rate limit read from every response's headers. */

export type FetchLike = (input: string, init?: RequestInit) => Promise<Response>;

export interface RateState {
  remaining: number;
  /** Epoch seconds when the window resets. */
  reset: number;
  resource: string;
}

export class GitHubError extends Error {
  constructor(
    message: string,
    readonly status = 0,
    readonly type = "",
  ) {
    super(message);
    this.name = "GitHubError";
  }
}

export interface GitHubOptions {
  token?: string;
  fetch?: FetchLike;
  /** Sleep hook (tests replace it); default Bun.sleep. */
  sleep?: (ms: number) => Promise<void>;
  /** Epoch seconds hook (tests). */
  now?: () => number;
  /** Stop spending the budget when this many calls are left in the window. */
  reserve?: number;
  retries?: number;
  log?: (line: string) => void;
  api?: string;
}

export const USER_AGENT = "aphrody-org-mirror/0.1 (+https://github.com/aphrody-labs/ay)";

/** A token from GH_TOKEN / GITHUB_TOKEN, else the one `gh auth token` holds. */
export function discoverToken(env = process.env): string | undefined {
  const fromEnv = env.GH_TOKEN || env.GITHUB_TOKEN;
  if (fromEnv) return fromEnv;
  const r = Bun.spawnSync(["gh", "auth", "token"], { stdout: "pipe", stderr: "ignore" });
  const out = r.exitCode === 0 ? r.stdout.toString().trim() : "";
  return out || undefined;
}

/** Whether a failure is worth retrying: transport errors, 5xx and the GraphQL "query timed out" shapes. */
const transient = (e: unknown) => !(e instanceof GitHubError) || e.status >= 500 || e.type === "TIMEOUT" || e.type === "TRANSIENT";

export class GitHub {
  readonly rate: Record<string, RateState> = {};
  /** Total points the GraphQL calls of this client cost, from their rateLimit.cost. */
  cost = 0;
  calls = 0;
  private readonly o: Required<Omit<GitHubOptions, "token" | "log">> & Pick<GitHubOptions, "token" | "log">;

  constructor(o: GitHubOptions = {}) {
    this.o = {
      fetch: o.fetch ?? fetch,
      sleep: o.sleep ?? Bun.sleep,
      now: o.now ?? (() => Date.now() / 1000),
      reserve: o.reserve ?? 25,
      retries: o.retries ?? 5,
      api: o.api ?? "https://api.github.com",
      token: o.token,
      log: o.log,
    };
  }

  /** Wait out an exhausted window of `resource` before spending more of it. */
  private async gate(resource: string): Promise<void> {
    const r = this.rate[resource];
    if (!r || r.remaining > this.o.reserve) return;
    const wait = Math.max(0, r.reset - this.o.now()) + 1;
    this.o.log?.(`rate limit: ${resource} has ${r.remaining} left, waiting ${Math.ceil(wait)}s for the window`);
    await this.o.sleep(wait * 1000);
    delete this.rate[resource];
  }

  private note(res: Response, fallback: string): void {
    const remaining = res.headers.get("x-ratelimit-remaining");
    const reset = res.headers.get("x-ratelimit-reset");
    if (remaining === null || reset === null) return;
    const resource = res.headers.get("x-ratelimit-resource") ?? fallback;
    this.rate[resource] = { remaining: Number(remaining), reset: Number(reset), resource };
  }

  private async send(path: string, init: RequestInit, resource: string): Promise<Response> {
    let last: unknown;
    for (let attempt = 0; attempt <= this.o.retries; attempt++) {
      await this.gate(resource);
      if (attempt) await this.o.sleep(Math.min(60_000, 1000 * 2 ** attempt));
      try {
        this.calls++;
        const res = await this.o.fetch(`${this.o.api}${path}`, {
          ...init,
          headers: { "user-agent": USER_AGENT, accept: "application/vnd.github+json", ...(this.o.token ? { authorization: `Bearer ${this.o.token}` } : {}), ...(init.headers as Record<string, string>) },
          signal: AbortSignal.timeout(90_000),
        });
        this.note(res, resource);
        if (res.status === 403 || res.status === 429) {
          const text = await res.text();
          const retryAfter = Number(res.headers.get("retry-after"));
          const r = this.rate[resource];
          if (retryAfter > 0) {
            this.o.log?.(`secondary rate limit: retry-after ${retryAfter}s`);
            await this.o.sleep(retryAfter * 1000);
            last = new GitHubError(`${res.status} secondary rate limit`, res.status, "TRANSIENT");
            continue;
          }
          if (r && r.remaining <= 0) {
            r.remaining = 0;
            last = new GitHubError(`${res.status} rate limit exhausted`, res.status, "TRANSIENT");
            continue;
          }
          if (/rate limit|abuse/i.test(text)) {
            await this.o.sleep(60_000);
            last = new GitHubError(`${res.status} ${text.slice(0, 120)}`, res.status, "TRANSIENT");
            continue;
          }
          throw new GitHubError(`${res.status} ${path}: ${text.slice(0, 200)}`, res.status);
        }
        if (res.status >= 500) {
          last = new GitHubError(`${res.status} ${path}`, res.status, "TRANSIENT");
          continue;
        }
        return res;
      } catch (e) {
        if (!transient(e)) throw e;
        last = e;
      }
    }
    throw last instanceof Error ? last : new GitHubError(String(last));
  }

  /** One GraphQL call. Partial data with only NOT_FOUND/FORBIDDEN errors (deleted users, hidden fields) is returned as is. */
  async graphql<T>(query: string, variables: Record<string, unknown> = {}): Promise<T> {
    return (await this.exec<T>(query, variables, false)).data;
  }

  /** Like graphql, but data that came back with real errors (one item GitHub cannot resolve) is returned together with them. */
  graphqlPartial<T>(query: string, variables: Record<string, unknown> = {}): Promise<{ data: T; errors: { type?: string; message: string }[] }> {
    return this.exec<T>(query, variables, true);
  }

  private async exec<T>(query: string, variables: Record<string, unknown>, partial: boolean): Promise<{ data: T; errors: { type?: string; message: string }[] }> {
    for (let attempt = 0; ; attempt++) {
      const res = await this.send("/graphql", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ query, variables }) }, "graphql");
      if (res.status === 401) throw new GitHubError("401 bad credentials", 401);
      const body = (await res.json().catch(() => null)) as { data?: T & { rateLimit?: { cost: number } }; errors?: { type?: string; message: string }[] } | null;
      if (!body) {
        if (attempt < 4) {
          await this.o.sleep(2000 * (attempt + 1));
          continue;
        }
        throw new GitHubError(`empty GraphQL response (HTTP ${res.status})`, 502, "TRANSIENT");
      }
      const errors = body.errors ?? [];
      const limited = errors.find(e => e.type === "RATE_LIMITED");
      if (limited && attempt < 3) {
        const r = this.rate.graphql;
        if (r) r.remaining = 0;
        await this.gate("graphql");
        continue;
      }
      const hard = errors.filter(e => e.type !== "NOT_FOUND" && e.type !== "FORBIDDEN");
      if (hard.length || !body.data) {
        const msg = (hard[0] ?? errors[0])?.message ?? `HTTP ${res.status}`;
        const timeout = /timeout|timed out|something went wrong|stream|resource limits|exceeded/i.test(msg);
        if (timeout && attempt < 2) {
          await this.o.sleep(2000 * (attempt + 1));
          continue;
        }
        if (partial && body.data) return { data: body.data, errors: hard };
        throw new GitHubError(msg, res.status, timeout ? "TIMEOUT" : (hard[0]?.type ?? ""));
      }
      this.cost += body.data.rateLimit?.cost ?? 1;
      return { data: body.data, errors: [] };
    }
  }

  /** One REST GET returning parsed JSON; a 404 returns undefined. */
  async rest<T>(path: string): Promise<T | undefined> {
    const res = await this.send(path, { method: "GET" }, "core");
    if (res.status === 404) return undefined;
    if (!res.ok) throw new GitHubError(`${res.status} ${path}`, res.status);
    return (await res.json()) as T;
  }
}

/** Run `fn` over pages while it returns a next cursor, halving the page size on query timeouts. `each` gets the page and the cursor after it and stops the crawl by returning false. */
export async function paginate<T>(first: number, fn: (after: string | null, first: number) => Promise<{ items: T[]; next: string | null }>, each: (items: T[], next: string | null) => boolean | void | Promise<boolean | void>, after: string | null = null): Promise<void> {
  let size = first;
  let cursor = after;
  for (;;) {
    let page;
    try {
      page = await fn(cursor, size);
    } catch (e) {
      if (e instanceof GitHubError && (e.type === "TIMEOUT" || e.status >= 500) && size > 1) {
        size = Math.max(1, Math.floor(size / 2));
        continue;
      }
      throw e;
    }
    if ((await each(page.items, page.next)) === false || !page.next) return;
    size = Math.min(first, size * 2);
    cursor = page.next;
  }
}
