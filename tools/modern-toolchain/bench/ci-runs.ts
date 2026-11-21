#!/usr/bin/env bun
// Tally the runs of one workflow by conclusion from the live GitHub API.
//   bun tools/modern-toolchain/bench/ci-runs.ts [--repo owner/name] [--workflow ci.yml] [--days 7] [--latest 51]
// The window is created >= now - days; --latest N tallies the N most recent runs instead. Output is JSON.
const args = process.argv.slice(2);
const option = (name: string, fallback: string) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? (args[index + 1] ?? fallback) : fallback;
};
const repo = option("repo", "aphrody-labs/fw");
const workflow = option("workflow", "ci.yml");
const days = Number(option("days", "7"));
const latest = Number(option("latest", "0"));

type Run = { created_at: string; status: string; conclusion: string | null; event: string };
const since = new Date(Date.now() - days * 86_400_000).toISOString();
const run = Bun.spawn(
  ["gh", "api", "--paginate", `repos/${repo}/actions/workflows/${workflow}/runs?per_page=100&created=>=${since.slice(0, 10)}`, "--jq", ".workflow_runs[] | {created_at,status,conclusion,event}"],
  { stdout: "pipe", stderr: "inherit" },
);
const text = await new Response(run.stdout).text();
if ((await run.exited) !== 0) process.exit(1);
let runs = text.split("\n").filter(Boolean).map((line) => JSON.parse(line) as Run);
runs.sort((a, b) => b.created_at.localeCompare(a.created_at));
if (latest > 0) runs = runs.slice(0, latest);
else runs = runs.filter((entry) => entry.created_at >= since);

const tally: Record<string, number> = {};
for (const entry of runs) {
  const key = entry.conclusion ?? entry.status;
  tally[key] = (tally[key] ?? 0) + 1;
}
console.log(JSON.stringify({ repo, workflow, window: latest > 0 ? `latest ${latest}` : `created >= ${since}`, total: runs.length, oldest: runs.at(-1)?.created_at, newest: runs[0]?.created_at, tally }, null, 2));
