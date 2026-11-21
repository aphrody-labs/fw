import { describe, expect, test } from "bun:test";
import { remoteScript, shipList } from "./compose-check.ts";

describe("compose-check", () => {
  test("ships the Gradle workspace, the tokens and the two manifests it reads, never build output", () => {
    const files = shipList();
    expect(files).toContain("kotlin/settings.gradle.kts");
    expect(files).toContain("kotlin/gradle/wrapper/gradle-wrapper.properties");
    expect(files).toContain("brand/tokens/m3-tokens.json");
    expect(files).toContain("brand/tokens/compose/AphrodyColorScheme.kt");
    expect(files).toContain("ay.toml");
    expect(files).toContain("Cargo.toml");
    expect(files.some((f) => /\/build\/|\/\.gradle\//.test(f))).toBe(false);
  });

  test("the remote script takes every host path from its arguments", () => {
    const script = remoteScript("/w/dir", { android_sdk: "/sdk", gradle_home: "/gh" }, "/w", ["test"]);
    expect(script).toContain("export GRADLE_USER_HOME='/gh'");
    expect(script).toContain("export ANDROID_HOME='/sdk'");
    expect(script).toContain("cd '/w/dir'/kotlin");
    expect(script).toContain("bash ./gradlew --no-daemon --console=plain 'test'");
    expect(remoteScript("/w/dir", {}, "/w", ["test"])).toContain("GRADLE_USER_HOME='/w/gradle-home'");
  });
});
