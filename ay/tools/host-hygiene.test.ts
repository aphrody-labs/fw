import { describe, expect, test } from "bun:test";
import { pinnedChannel, renderInstall, renderScript, renderUnits, resolvePolicy } from "./host-hygiene.ts";

describe("host-hygiene", () => {
  test("policy defaults, the pinned toolchain and local lists", () => {
    const p = resolvePolicy({ hygiene: { target_cap_gb: 10, protect: ["/opt/site"], systemd: { keep: ["fleet-*"] } } }, "1.100.0");
    expect(p.targetCapGb).toBe(10);
    expect(p.nodeModulesIdleDays).toBe(7);
    expect(p.keepToolchains).toEqual(["1.100.0"]);
    expect(p.protect).toEqual(["/opt/site"]);
    expect(p.systemdKeep).toEqual(["fleet-*"]);
    expect(pinnedChannel()).toMatch(/^\d+\.\d+\.\d+$/);
  });

  test("refuses unsafe or invalid values", () => {
    expect(() => resolvePolicy({ hygiene: { target_cap_gb: 0 } }, "x")).toThrow(/positive/);
    expect(() => resolvePolicy({ hygiene: { protect: ["/opt/a; rm -rf /"] } }, "x")).toThrow(/characters/);
    expect(() => resolvePolicy({ hygiene: { journal_max: "lots" } }, "x")).toThrow(/journal_max/);
  });

  test("the script is valid bash, dry by default and keeps volumes on the prod role", () => {
    const p = resolvePolicy({ hygiene: { protect: ["/opt/site"] } }, "1.100.0");
    const dry = renderScript(p, "prod", false);
    expect(dry).toContain("APPLY=0");
    expect(dry).toContain("PROTECT=('/opt/site')");
    expect(dry).toContain("ROLE='prod'");
    expect(renderScript(p, "build", true, true)).toContain("DISABLE_UNDECLARED=1");
    const check = Bun.spawnSync(["bash", "-n"], { stdin: new Blob([dry]) });
    expect(check.stderr.toString()).toBe("");
    expect(check.exitCode).toBe(0);
  });

  test("the install script writes the script and the timer, and replaces fleet-clean", () => {
    const text = renderInstall(resolvePolicy({}, "1.100.0"), "build");
    expect(text).toContain("/usr/local/bin/ay-hygiene");
    expect(text).toContain("disable --now fleet-clean.timer");
    expect(text).toContain("enable --now ay-hygiene.timer");
    expect(Bun.spawnSync(["bash", "-n"], { stdin: new Blob([text]) }).exitCode).toBe(0);
  });

  test("the timer runs every 30 minutes at idle priority", () => {
    const u = renderUnits();
    expect(u.timer).toContain("OnUnitActiveSec=30min");
    expect(u.service).toContain("IOSchedulingClass=idle");
  });
});
