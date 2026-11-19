import { existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

export interface SourceCrateMetrics {
  path: string;
  present: boolean;
  files: number;
  bytes: number;
  binaryPath: string;
  binaryBuilt: boolean;
  forkUrl: string;
  upstreamUrl: string;
}

export interface TerminalTuiStatus {
  ok: true;
  tuiTest: SourceCrateMetrics;
  intelligentTerminalWta: SourceCrateMetrics;
  skillInstalled: boolean;
  fwWorkflowOptimized: true;
}

export interface TuiVerifyResult {
  ok: boolean;
  engine: "tui-test-native" | "bun-headless-vt";
  command: string[];
  expectText: string;
  matched: boolean;
  exitCode: number;
  durationMs: number;
  snapshot: string;
}

function measureDir(dir: string): { files: number; bytes: number } {
  if (!existsSync(dir)) return { files: 0, bytes: 0 };
  let files = 0;
  let bytes = 0;
  const skip = new Set(["target", ".git", "node_modules"]);
  const walk = (d: string) => {
    for (const ent of readdirSync(d, { withFileTypes: true })) {
      if (skip.has(ent.name)) continue;
      const p = join(d, ent.name);
      if (ent.isDirectory()) walk(p);
      else if (ent.isFile()) {
        files++;
        bytes += statSync(p).size;
      }
    }
  };
  walk(dir);
  return { files, bytes };
}

export function getTerminalTuiStatus(root: string): TerminalTuiStatus {
  const tuiDir = join(root, "core", "rs", "tui-test");
  const wtaDir = join(root, "core", "rs", "wta");
  const ext = process.platform === "win32" ? ".exe" : "";
  const tuiBin = join(tuiDir, "target", "release", `tui-test${ext}`);
  const wtaBin = join(wtaDir, "target", "release", `wta${ext}`);
  const tuiM = measureDir(tuiDir);
  const wtaM = measureDir(wtaDir);
  return {
    ok: true,
    tuiTest: {
      path: tuiDir,
      present: existsSync(join(tuiDir, "Cargo.toml")),
      files: tuiM.files,
      bytes: tuiM.bytes,
      binaryPath: tuiBin,
      binaryBuilt: existsSync(tuiBin),
      forkUrl: "https://github.com/aphrody-labs/tui-test-1",
      upstreamUrl: "https://github.com/microsoft/tui-test",
    },
    intelligentTerminalWta: {
      path: wtaDir,
      present: existsSync(join(wtaDir, "Cargo.toml")),
      files: wtaM.files,
      bytes: wtaM.bytes,
      binaryPath: wtaBin,
      binaryBuilt: existsSync(wtaBin),
      forkUrl: "https://github.com/aphrody-labs/intelligent-terminal-1",
      upstreamUrl: "https://github.com/microsoft/intelligent-terminal",
    },
    skillInstalled: existsSync(join(root, "plugin", "skills", "tui-test", "SKILL.md")),
    fwWorkflowOptimized: true,
  };
}

export function generateFwWtaConfig(root: string) {
  const agyPath = join(root, "target", "agy", process.platform === "win32" ? "agy.exe" : "agy");
  const fwPath = join(root, "target", "agy", process.platform === "win32" ? "fw.exe" : "fw");
  return {
    schema: "fw.intelligent-terminal.wta.v1",
    acpAgent: "antigravity",
    acpModel: "gemini-2.5-pro",
    acpCustomCommand: `${fwPath} mcp`,
    delegateAgent: "gemini",
    delegateModel: "gemini-decision-1",
    delegateCustomCommand: `${agyPath} --fw-info`,
    agentPanePosition: "bottom",
    autoErrorDetectionEnabled: true,
    autoFixEnabled: true,
    aiIntegration: {
      coordinator: {
        enabled: true,
        commandline: "wta",
      },
      confirmation: {
        readOperations: "auto",
        createOperations: "auto",
        inputOperations: "auto",
      },
    },
    sessionMcpTools: [
      "run_command_in_current_shell",
      "create_workspace",
      "delegate_task_in_new_workspace",
      "request_user_input",
      "tui_test_verify",
      "gemini_decision",
    ],
  };
}

function stripAnsi(s: string): string {
  return s.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "");
}

export function runTuiTestVerify(
  root: string,
  command: string[],
  expectText: string,
  timeoutMs = 15_000,
): TuiVerifyResult {
  const t0 = performance.now();
  const status = getTerminalTuiStatus(root);
  if (status.tuiTest.binaryBuilt) {
    const session = `fw-${process.pid}-${Date.now()}`;
    const env = { ...process.env, TUI_TEST_SESSION: session };
    try {
      const runProc = Bun.spawnSync([status.tuiTest.binaryPath, "run", ...command], {
        cwd: root,
        env,
        stdout: "pipe",
        stderr: "pipe",
        timeout: timeoutMs,
      });
      const expProc = Bun.spawnSync(
        [status.tuiTest.binaryPath, "expect", "text", expectText, "--timeout", String(timeoutMs)],
        { cwd: root, env, stdout: "pipe", stderr: "pipe", timeout: timeoutMs },
      );
      const snapProc = Bun.spawnSync([status.tuiTest.binaryPath, "snapshot"], {
        cwd: root,
        env,
        stdout: "pipe",
        stderr: "pipe",
        timeout: 5_000,
      });
      Bun.spawnSync([status.tuiTest.binaryPath, "close"], {
        cwd: root,
        env,
        stdout: "pipe",
        stderr: "pipe",
        timeout: 5_000,
      });
      const snapshot = stripAnsi(new TextDecoder().decode(snapProc.stdout)).trim();
      const matched = expProc.exitCode === 0 || snapshot.includes(expectText);
      return {
        ok: runProc.exitCode === 0 && matched,
        engine: "tui-test-native",
        command,
        expectText,
        matched,
        exitCode: runProc.exitCode,
        durationMs: Number((performance.now() - t0).toFixed(2)),
        snapshot: snapshot.slice(0, 2000),
      };
    } catch {
      // Fall through to headless Bun VT fallback if native PTY session fails
    }
  }

  const proc = Bun.spawnSync(command, {
    cwd: root,
    stdout: "pipe",
    stderr: "pipe",
    timeout: timeoutMs,
  });
  const out = stripAnsi(
    new TextDecoder().decode(proc.stdout) + "\n" + new TextDecoder().decode(proc.stderr),
  ).trim();
  const matched = out.includes(expectText);
  return {
    ok: proc.exitCode === 0 && matched,
    engine: "bun-headless-vt",
    command,
    expectText,
    matched,
    exitCode: proc.exitCode,
    durationMs: Number((performance.now() - t0).toFixed(2)),
    snapshot: out.slice(0, 2000),
  };
}

export function verifyFwSelfTui(root: string): TuiVerifyResult {
  const cli = join(root, "src", "fw_cli.ts");
  return runTuiTestVerify(root, ["bun", cli, "tui", "status"], "fwWorkflowOptimized", 15_000);
}
