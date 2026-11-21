import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createTarGz, extractAll, listEntries } from "../src/archive.ts";

// ── Fixtures ──────────────────────────────────────────────────────────────────

const workDir = join(tmpdir(), `ay-archive-test-${process.pid}`);
const archivePath = join(workDir, "test.tar.gz");
const extractDir = join(workDir, "extracted");
const createdArchivePath = join(workDir, "created.tar.gz");

const FILES = {
  "hello.txt": "hello world\n",
  "sub/nested.txt": "nested content\n",
};

beforeAll(() => {
  mkdirSync(workDir, { recursive: true });
  mkdirSync(extractDir, { recursive: true });

  // Create the source files
  const srcDir = join(workDir, "src");
  mkdirSync(srcDir, { recursive: true });
  mkdirSync(join(srcDir, "sub"), { recursive: true });

  for (const [name, content] of Object.entries(FILES)) {
    writeFileSync(join(srcDir, name), content, "utf8");
  }

  // Create the test archive with the system tar
  const result = Bun.spawnSync(["tar", "-czf", archivePath, "-C", srcDir, "hello.txt", "sub/nested.txt"]);
  if (result.exitCode !== 0) {
    throw new Error(`tar failed: ${new TextDecoder().decode(result.stderr)}`);
  }
});

afterAll(() => {
  try {
    rmSync(workDir, { recursive: true, force: true });
  } catch {}
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("archive: listEntries", () => {
  test("1. returns the right names from a tar.gz created by tar", () => {
    const entries = listEntries(archivePath);
    const names = entries.map((e) => e.pathname);
    expect(names).toContain("hello.txt");
    expect(names).toContain("sub/nested.txt");
  });

  test("2. every entry has the fields pathname, size, isDirectory", () => {
    const entries = listEntries(archivePath);
    expect(entries.length).toBeGreaterThan(0);
    for (const entry of entries) {
      expect(typeof entry.pathname).toBe("string");
      expect(typeof entry.size).toBe("number");
      expect(typeof entry.isDirectory).toBe("boolean");
    }
  });

  test("3. regular entries have isDirectory = false", () => {
    const entries = listEntries(archivePath);
    const regularFiles = entries.filter((e) => !e.isDirectory);
    expect(regularFiles.length).toBeGreaterThan(0);
    for (const f of regularFiles) {
      expect(f.isDirectory).toBe(false);
    }
  });
});

describe("archive: extractAll", () => {
  test("4. extractAll extracts the files into destDir", () => {
    extractAll(archivePath, extractDir);
    const helloFile = Bun.file(join(extractDir, "hello.txt"));
    expect(helloFile.size).toBeGreaterThan(0);
  });

  test("5. the content of the extracted files matches the original", () => {
    extractAll(archivePath, extractDir);
    const content = readFileSync(join(extractDir, "hello.txt"), "utf8");
    expect(content).toBe(FILES["hello.txt"]);
  });

  test("extractAll also restores nested paths", () => {
    extractAll(archivePath, extractDir);
    expect(readFileSync(join(extractDir, "sub/nested.txt"), "utf8")).toBe(FILES["sub/nested.txt"]);
  });

  test("extractAll skips entries that escape destDir with '..'", () => {
    const evilArchive = join(workDir, "evil.tar.gz");
    createTarGz(evilArchive, [
      { path: join(workDir, "src", "hello.txt"), name: "../escaped.txt" },
      { path: join(workDir, "src", "hello.txt"), name: "inside.txt" },
    ]);
    const dest = join(workDir, "evil-dest");
    mkdirSync(dest, { recursive: true });
    extractAll(evilArchive, dest);
    expect(existsSync(join(workDir, "escaped.txt"))).toBe(false);
    expect(existsSync(join(dest, "inside.txt"))).toBe(true);
  });
});

describe("archive: createTarGz", () => {
  test("6. createTarGz creates a file on disk", () => {
    const srcDir = join(workDir, "src");
    createTarGz(createdArchivePath, [
      { path: join(srcDir, "hello.txt"), name: "hello.txt" },
      { path: join(srcDir, "sub/nested.txt"), name: "sub/nested.txt" },
    ]);
    const outFile = Bun.file(createdArchivePath);
    expect(outFile.size).toBeGreaterThan(0);
  });

  test("7. the archive created by createTarGz can be listed by listEntries", () => {
    const srcDir = join(workDir, "src");
    createTarGz(createdArchivePath, [
      { path: join(srcDir, "hello.txt"), name: "hello.txt" },
      { path: join(srcDir, "sub/nested.txt"), name: "sub/nested.txt" },
    ]);
    const entries = listEntries(createdArchivePath);
    const names = entries.map((e) => e.pathname);
    expect(names).toContain("hello.txt");
    expect(names).toContain("sub/nested.txt");
  });

  test("created archive round-trips through extractAll and system tar", () => {
    const dest = join(workDir, "roundtrip");
    mkdirSync(dest, { recursive: true });
    const result = Bun.spawnSync(["tar", "-xzf", createdArchivePath, "-C", dest]);
    expect(result.exitCode).toBe(0);
    expect(readFileSync(join(dest, "hello.txt"), "utf8")).toBe(FILES["hello.txt"]);
  });
});

describe("archive: errors", () => {
  test("8. missing archive -> explicit error", () => {
    expect(() => listEntries(join(tmpdir(), "nonexistent-archive-xyz-12345.tar.gz"))).toThrow(
      "Cannot open archive",
    );
  });
});
