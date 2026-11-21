#!/usr/bin/env bun
// Duration of every job of the latest successful or failed runs (cancelled runs have no jobs) of one workflow, per job name, from the live GitHub API.
//   bun tools/modern-toolchain/bench/ci-jobs.ts [--repo owner/name] [--workflow ci.yml] [--runs 20]
// A job that never started (cancelled while queued) has no duration and is counted apart.
const args = process.argv.slice(2);
const option = (name: string, fallback: string) => {
  const index = args.indexOf(`--${name}`);
  return index >= 0 ? (args[index + 1] ?? fallback) : fallback;
};
const repo = option("repo", "aphrody-labs/fw");
const workflow = option("workflow", "ci.yml");
const wanted = Number(option("runs", "20"));

const gh = async (path: string, jq: string) => {
  const run = Bun.spawn(["gh", "api", path, "--jq", jq], { stdout: "pipe", stderr: "inherit" });
  const text = await new Response(run.stdout).text();
  if ((await run.exited) !== 0) process.exit(1);
  return text.split("\n").filter(Boolean).map((line) => JSON.parse(line));
};

const runs = (await gh(`repos/${repo}/actions/workflows/${workflow}/runs?per_page=100&status=completed`, '.workflow_runs[] | select(.conclusion == "success" or .conclusion == "failure") | {id,conclusion,created_at}')).slice(0, wanted);
const durations = new Map<string, { seconds: number[]; conclusions: Record<string, number>; runners: Set<string>; unstarted: number }>();
for (const entry of runs) {
  const jobs = await gh(`repos/${repo}/actions/runs/${entry.id}/jobs?per_page=100`, ".jobs[] | {name,conclusion,started_at,completed_at,runner_name,labels}");
  for (const job of jobs) {
    const slot = durations.get(job.name) ?? { seconds: [], conclusions: {}, runners: new Set<string>(), unstarted: 0 };
    durations.set(job.name, slot);
    slot.conclusions[job.conclusion ?? "none"] = (slot.conclusions[job.conclusion ?? "none"] ?? 0) + 1;
    if (!job.started_at || !job.completed_at || job.conclusion === "skipped") {
      slot.unstarted++;
      continue;
    }
    slot.seconds.push((Date.parse(job.completed_at) - Date.parse(job.started_at)) / 1000);
    for (const label of job.labels ?? []) slot.runners.add(label);
  }
}
const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
console.log(JSON.stringify({ repo, workflow, runs: runs.length, oldest: runs.at(-1)?.created_at, newest: runs[0]?.created_at }));
for (const [name, slot] of durations) {
  console.log(JSON.stringify({ job: name, ran: slot.seconds.length, unstarted: slot.unstarted, min_s: Math.min(...slot.seconds), median_s: median(slot.seconds), max_s: Math.max(...slot.seconds), conclusions: slot.conclusions, labels: [...slot.runners] }));
}
