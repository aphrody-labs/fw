import { appendFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { applyUpgrade, checkUpgrade } from "./upgrade.ts";
import { syncFwPlugin } from "./plugin_sync.ts";

export const WINDOWS_TASK_NAME = "FwAgyContinuousUpgrade";
export const LINUX_SERVICE_NAME = "fw-agy-upgrade.service";
export const MACOS_LAUNCHD_LABEL = "com.aphrody.fw-agy";

export interface DaemonCycleOptions {
  autoApply?: boolean;
  withLearn?: boolean;
  syncPlugin?: boolean;
  skipTests?: boolean;
}

export interface DaemonCycleResult {
  ok: boolean;
  timestamp: string;
  root: string;
  upgradeRequired: boolean;
  changedModules: string[];
  appliedUpgrade: boolean;
  learnDistilled: boolean;
  pluginSynced: boolean;
  errors: string[];
  statePath: string;
}

function xmlEscape(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function resolveServiceExecutable(root: string): { command: string; args: string[] } {
  const isWin = process.platform === "win32";
  const cliTarget = resolve(root, "target", "agy", isWin ? "fw.exe" : "fw");
  const cliDist = resolve(root, "dist", isWin ? "fw.exe" : "fw");
  if (existsSync(cliTarget)) {
    return { command: cliTarget, args: ["daemon", "run", "--interval=300", "--auto-apply", "--with-learn"] };
  }
  if (existsSync(cliDist)) {
    return { command: cliDist, args: ["daemon", "run", "--interval=300", "--auto-apply", "--with-learn"] };
  }
  return {
    command: "bun",
    args: [resolve(root, "src", "fw_cli.ts"), "daemon", "run", "--interval=300", "--auto-apply", "--with-learn"],
  };
}

export function renderCrossPlatformServices(root: string) {
  const serviceRoot = resolve(root, "target", "service");
  const winDir = join(serviceRoot, "windows");
  const linuxDir = join(serviceRoot, "linux");
  const macDir = join(serviceRoot, "macos");
  mkdirSync(winDir, { recursive: true });
  mkdirSync(linuxDir, { recursive: true });
  mkdirSync(macDir, { recursive: true });

  const winExe = resolve(root, "target", "agy", "fw.exe");
  const winArgs = "daemon run --interval=300 --auto-apply --with-learn";
  const winXml = `<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Fw continuous LTO/AOT self-upgrade and autonomous learning loop for agy</Description>
    <Author>fw</Author>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>true</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>${xmlEscape(winExe)}</Command>
      <Arguments>${xmlEscape(winArgs)}</Arguments>
      <WorkingDirectory>${xmlEscape(root)}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
`;

  const unixRoot = root.replaceAll("\\", "/");
  const linuxExec = `${unixRoot}/target/agy/fw daemon run --interval=300 --auto-apply --with-learn`;
  const linuxUnit = `[Unit]
Description=Fw Continuous LTO/AOT Self-Upgrade & Learning Daemon for agy
After=network-online.target

[Service]
Type=simple
WorkingDirectory=${unixRoot}
ExecStart=${linuxExec}
Restart=always
RestartSec=5
Environment=FW_ROOT=${unixRoot}

[Install]
WantedBy=default.target
`;

  const macPlist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${MACOS_LAUNCHD_LABEL}</string>
  <key>WorkingDirectory</key>
  <string>${xmlEscape(unixRoot)}</string>
  <key>ProgramArguments</key>
  <array>
    <string>${xmlEscape(`${unixRoot}/target/agy/fw`)}</string>
    <string>daemon</string>
    <string>run</string>
    <string>--interval=300</string>
    <string>--auto-apply</string>
    <string>--with-learn</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>FW_ROOT</key>
    <string>${xmlEscape(unixRoot)}</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>StandardOutPath</key>
  <string>${xmlEscape(`${unixRoot}/target/daemon.stdout.log`)}</string>
  <key>StandardErrorPath</key>
  <string>${xmlEscape(`${unixRoot}/target/daemon.stderr.log`)}</string>
</dict>
</plist>
`;

  const winXmlPath = join(winDir, `${WINDOWS_TASK_NAME}.xml`);
  const linuxUnitPath = join(linuxDir, LINUX_SERVICE_NAME);
  const macPlistPath = join(macDir, `${MACOS_LAUNCHD_LABEL}.plist`);

  // Task Scheduler XML on Windows expects UTF-16LE with BOM
  writeFileSync(winXmlPath, Buffer.concat([Buffer.from([0xff, 0xfe]), Buffer.from(winXml, "utf16le")]));
  writeFileSync(linuxUnitPath, linuxUnit, "utf8");
  writeFileSync(macPlistPath, macPlist, "utf8");

  return {
    ok: true,
    platform: process.platform,
    manifests: {
      windows: winXmlPath,
      linux: linuxUnitPath,
      macos: macPlistPath,
    },
    executable: resolveServiceExecutable(root),
  };
}

export function installService(root: string) {
  const rendered = renderCrossPlatformServices(root);
  if (process.platform === "win32") {
    const res = Bun.spawnSync(
      ["schtasks.exe", "/Create", "/TN", WINDOWS_TASK_NAME, "/XML", rendered.manifests.windows, "/F"],
      { cwd: root, stdout: "pipe", stderr: "pipe" },
    );
    return {
      ok: res.exitCode === 0,
      platform: "win32",
      serviceName: WINDOWS_TASK_NAME,
      manifest: rendered.manifests.windows,
      bootStartup: true,
      autoRestart: true,
      stdout: res.stdout.toString().trim(),
      stderr: res.stderr.toString().trim() || undefined,
    };
  }
  if (process.platform === "linux") {
    const unitDst = join(homedir(), ".config", "systemd", "user", LINUX_SERVICE_NAME);
    mkdirSync(dirname(unitDst), { recursive: true });
    writeFileSync(unitDst, readFileSync(rendered.manifests.linux, "utf8"), "utf8");
    const reload = Bun.spawnSync(["systemctl", "--user", "daemon-reload"], { stdout: "pipe", stderr: "pipe" });
    const enable = Bun.spawnSync(["systemctl", "--user", "enable", "--now", LINUX_SERVICE_NAME], {
      stdout: "pipe",
      stderr: "pipe",
    });
    return {
      ok: reload.exitCode === 0 && enable.exitCode === 0,
      platform: "linux",
      serviceName: LINUX_SERVICE_NAME,
      unitPath: unitDst,
      bootStartup: true,
      autoRestart: true,
      stderr: (enable.stderr.toString() || reload.stderr.toString()).trim() || undefined,
    };
  }
  if (process.platform === "darwin") {
    const plistDst = join(homedir(), "Library", "LaunchAgents", `${MACOS_LAUNCHD_LABEL}.plist`);
    mkdirSync(dirname(plistDst), { recursive: true });
    writeFileSync(plistDst, readFileSync(rendered.manifests.macos, "utf8"), "utf8");
    const load = Bun.spawnSync(["launchctl", "load", "-w", plistDst], { stdout: "pipe", stderr: "pipe" });
    return {
      ok: load.exitCode === 0,
      platform: "darwin",
      serviceName: MACOS_LAUNCHD_LABEL,
      plistPath: plistDst,
      bootStartup: true,
      autoRestart: true,
      stderr: load.stderr.toString().trim() || undefined,
    };
  }
  return { ok: false, platform: process.platform, error: "Unsupported platform" };
}

export function uninstallService(root: string) {
  if (process.platform === "win32") {
    const res = Bun.spawnSync(["schtasks.exe", "/Delete", "/TN", WINDOWS_TASK_NAME, "/F"], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    });
    return {
      ok: res.exitCode === 0,
      platform: "win32",
      serviceName: WINDOWS_TASK_NAME,
      stdout: res.stdout.toString().trim(),
      stderr: res.stderr.toString().trim() || undefined,
    };
  }
  if (process.platform === "linux") {
    Bun.spawnSync(["systemctl", "--user", "disable", "--now", LINUX_SERVICE_NAME], { stdout: "pipe", stderr: "pipe" });
    const unitDst = join(homedir(), ".config", "systemd", "user", LINUX_SERVICE_NAME);
    if (existsSync(unitDst)) rmSync(unitDst, { force: true });
    return { ok: true, platform: "linux", serviceName: LINUX_SERVICE_NAME };
  }
  if (process.platform === "darwin") {
    const plistDst = join(homedir(), "Library", "LaunchAgents", `${MACOS_LAUNCHD_LABEL}.plist`);
    Bun.spawnSync(["launchctl", "unload", "-w", plistDst], { stdout: "pipe", stderr: "pipe" });
    if (existsSync(plistDst)) rmSync(plistDst, { force: true });
    return { ok: true, platform: "darwin", serviceName: MACOS_LAUNCHD_LABEL };
  }
  return { ok: false, platform: process.platform, error: "Unsupported platform" };
}

export function serviceStatus(root: string) {
  const statePath = resolve(root, "target", "daemon-state.json");
  let lastState: unknown = null;
  if (existsSync(statePath)) {
    try {
      lastState = JSON.parse(readFileSync(statePath, "utf8"));
    } catch {
      lastState = null;
    }
  }
  const manifestsPresent = {
    windows: existsSync(resolve(root, "target", "service", "windows", `${WINDOWS_TASK_NAME}.xml`)),
    linux: existsSync(resolve(root, "target", "service", "linux", LINUX_SERVICE_NAME)),
    macos: existsSync(resolve(root, "target", "service", "macos", `${MACOS_LAUNCHD_LABEL}.plist`)),
  };

  let installed = false;
  let detail = "";
  if (process.platform === "win32") {
    const q = Bun.spawnSync(["schtasks.exe", "/Query", "/TN", WINDOWS_TASK_NAME, "/FO", "LIST"], {
      stdout: "pipe",
      stderr: "pipe",
    });
    installed = q.exitCode === 0;
    detail = installed ? q.stdout.toString().trim().split(/\r?\n/).slice(0, 8).join(" | ") : "not registered";
  } else if (process.platform === "linux") {
    const unitDst = join(homedir(), ".config", "systemd", "user", LINUX_SERVICE_NAME);
    installed = existsSync(unitDst);
    detail = installed ? unitDst : "not installed";
  } else if (process.platform === "darwin") {
    const plistDst = join(homedir(), "Library", "LaunchAgents", `${MACOS_LAUNCHD_LABEL}.plist`);
    installed = existsSync(plistDst);
    detail = installed ? plistDst : "not installed";
  }

  return {
    ok: true,
    platform: process.platform,
    installed,
    detail,
    manifestsPresent,
    statePath,
    lastState,
  };
}

export function runDaemonCycle(root: string, opts: DaemonCycleOptions = {}): DaemonCycleResult {
  const timestamp = new Date().toISOString();
  const errors: string[] = [];
  const check = checkUpgrade(root);
  let appliedUpgrade = false;
  let learnDistilled = false;
  let pluginSynced = false;

  if (check.upgradeRequired && opts.autoApply) {
    const applied = applyUpgrade(root, { skipTests: opts.skipTests ?? false });
    appliedUpgrade = applied.ok;
    if (!applied.ok && applied.error) errors.push(applied.error);
  }

  if (opts.withLearn !== false) {
    const learnScript = join(homedir(), ".gemini", "scripts", "learn.ts");
    if (existsSync(learnScript)) {
      const res = Bun.spawnSync(["bun", learnScript, "distill-quiet"], { stdout: "pipe", stderr: "pipe" });
      learnDistilled = res.exitCode === 0;
      if (!learnDistilled) errors.push(`learn distill-quiet failed (exit ${res.exitCode})`);
    }
  }

  if (opts.syncPlugin) {
    const syncRes = syncFwPlugin(root, { runInstall: false });
    pluginSynced = syncRes.ok;
    if (!syncRes.ok && syncRes.error) errors.push(syncRes.error);
  }

  const statePath = resolve(root, "target", "daemon-state.json");
  const logPath = resolve(root, "target", "daemon.jsonl");
  mkdirSync(dirname(statePath), { recursive: true });

  const result: DaemonCycleResult = {
    ok: errors.length === 0,
    timestamp,
    root,
    upgradeRequired: check.upgradeRequired,
    changedModules: check.changedModules,
    appliedUpgrade,
    learnDistilled,
    pluginSynced,
    errors,
    statePath,
  };

  writeFileSync(statePath, `${JSON.stringify(result, null, 2)}\n`, "utf8");
  appendFileSync(logPath, `${JSON.stringify(result)}\n`, "utf8");
  return result;
}

export async function runDaemonLoop(
  root: string,
  opts: DaemonCycleOptions & { intervalSec?: number; once?: boolean } = {},
): Promise<number> {
  const intervalMs = Math.max(5, opts.intervalSec ?? 300) * 1000;
  let stopped = false;
  process.on("SIGINT", () => {
    stopped = true;
  });
  process.on("SIGTERM", () => {
    stopped = true;
  });

  let backoffMs = 5000;
  while (!stopped) {
    const res = runDaemonCycle(root, opts);
    console.log(JSON.stringify(res));
    if (opts.once) return res.ok ? 0 : 1;
    const sleepMs = res.ok ? intervalMs : Math.min(backoffMs, intervalMs);
    backoffMs = res.ok ? 5000 : Math.min(backoffMs * 2, 60000);
    await Bun.sleep(sleepMs);
  }
  return 0;
}
