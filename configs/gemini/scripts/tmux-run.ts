#!/usr/bin/env bun
/**
 * Deterministic remote tmux launcher & status checker for long-running fleet commands.
 * Never lets an SSH disconnect kill a build, kernel compile, sync, or installer.
 *
 * Usage:
 *   bun ~/.gemini/scripts/tmux-run.ts start <host> <job> <command>
 *   bun ~/.gemini/scripts/tmux-run.ts status <host> <job>
 *   bun ~/.gemini/scripts/tmux-run.ts list <host>
 */

export {};

const [action, host, job, ...rest] = Bun.argv.slice(2);

if (!action || !host) {
  console.error("Usage: bun tmux-run.ts <start|status|list> <host> [job] [command...]");
  process.exit(1);
}

async function sshRun(targetHost: string, script: string) {
  const proc = Bun.spawn(
    ["ssh", "-o", "ConnectTimeout=8", "-o", "BatchMode=yes", targetHost, "tr -d '\\r' | bash"],
    { stdin: new Response(script), stdout: "pipe", stderr: "pipe" },
  );
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  return { exitCode, stdout: stdout.trim(), stderr: stderr.trim() };
}

if (action === "list") {
  const res = await sshRun(host, `tmux ls 2>/dev/null || echo "(no active tmux sessions)"`);
  console.log(JSON.stringify({ host, ...res }, null, 2));
  process.exit(res.exitCode);
}

if (!job) {
  console.error("Missing <job> name.");
  process.exit(1);
}

const safeJob = job.replace(/[^a-zA-Z0-9_-]/g, "-");
const logFile = `/tmp/tmux-${safeJob}.log`;
const rcFile = `/tmp/tmux-${safeJob}.rc`;

if (action === "start") {
  const cmd = rest.join(" ").trim();
  if (!cmd) {
    console.error("Missing <command> for start.");
    process.exit(1);
  }
  const b64 = Buffer.from(cmd, "utf8").toString("base64");
  const script = `set -euo pipefail
tmux kill-session -t ${safeJob} 2>/dev/null || true
rm -f ${logFile} ${rcFile}
cat <<'WRAP' > /tmp/tmux-${safeJob}.sh
#!/usr/bin/env bash
set -o pipefail
CMD="$(printf '%s' '${b64}' | base64 -d)"
bash -lc "$CMD" 2>&1 | tee ${logFile}
rc=\${PIPESTATUS[0]}
echo "$rc" > ${rcFile}
exit "$rc"
WRAP
chmod +x /tmp/tmux-${safeJob}.sh
tmux new-session -d -s ${safeJob} "/tmp/tmux-${safeJob}.sh"
echo "STARTED session=${safeJob} log=${logFile}"
`;
  const res = await sshRun(host, script);
  console.log(JSON.stringify({ host, job: safeJob, logFile, rcFile, ...res }, null, 2));
  process.exit(res.exitCode);
}

if (action === "status") {
  const script = `
ALIVE=0
tmux has-session -t ${safeJob} 2>/dev/null && ALIVE=1
RC=$(cat ${rcFile} 2>/dev/null || echo "running")
echo "ALIVE=$ALIVE"
echo "RC=$RC"
echo "---TAIL---"
tail -n 30 ${logFile} 2>/dev/null || true
`;
  const res = await sshRun(host, script);
  const alive = /ALIVE=1/.test(res.stdout);
  const rcMatch = res.stdout.match(/RC=([^\r\n]+)/);
  const tail = res.stdout.split("---TAIL---\n")[1] ?? "";
  console.log(JSON.stringify({ host, job: safeJob, alive, exitCode: rcMatch?.[1] ?? "unknown", tail }, null, 2));
  process.exit(0);
}

console.error(`Unknown action: ${action}`);
process.exit(1);
