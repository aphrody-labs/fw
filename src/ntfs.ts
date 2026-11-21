import {
  copyFileSync,
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  renameSync,
  rmSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";

const NTFS_RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\.|$)/i;

/**
 * Adds Windows extended-length prefix (\\?\) when an absolute path approaches
 * the legacy MAX_PATH (260) limit, preventing ENOENT/ENAMETOOLONG on NTFS.
 */
export function toNtfsSafePath(inputPath: string): string {
  const abs = resolve(inputPath);
  if (process.platform !== "win32") return abs;
  if (abs.startsWith("\\\\?\\") || abs.startsWith("\\\\.\\")) return abs;
  if (abs.length < 240) return abs;
  if (abs.startsWith("\\\\")) {
    return `\\\\?\\UNC\\${abs.slice(2)}`;
  }
  return `\\\\?\\${abs}`;
}

export function isNtfsReservedName(segment: string): boolean {
  return NTFS_RESERVED.test(segment.trim());
}

export function normalizeNtfsKey(p: string): string {
  const normalized = p.replaceAll("\\", "/").replace(/\/+$/, "");
  return process.platform === "win32" ? normalized.toLowerCase() : normalized;
}

/**
 * Detects NTFS directory junctions, symbolic links, and reparse points without
 * following them (fixes the Go os.ReadDir junction-skip bug on Windows).
 */
export function isReparseOrSymlink(targetPath: string): boolean {
  try {
    const st = lstatSync(toNtfsSafePath(targetPath));
    return st.isSymbolicLink();
  } catch {
    return false;
  }
}

/**
 * Guarantees `dirPath` is a real directory (never an NTFS junction or symlink).
 */
export function ensureRealDirectory(dirPath: string): void {
  const safe = toNtfsSafePath(dirPath);
  if (isReparseOrSymlink(safe)) {
    rmSync(safe, { recursive: true, force: true });
  }
  mkdirSync(safe, { recursive: true });
}

/**
 * Mirrors `srcDir` into `dstDir` as 100% real NTFS files and directories,
 * dereferencing any symlinks/junctions and retrying transient Windows AV locks.
 */
export function mirrorRealDirNtfs(srcDir: string, dstDir: string): void {
  ensureRealDirectory(dstDir);
  const safeSrc = toNtfsSafePath(srcDir);
  const safeDst = toNtfsSafePath(dstDir);

  const srcEntries = readdirSync(safeSrc, { withFileTypes: true });
  const srcNameKeys = new Set(srcEntries.map((e) => normalizeNtfsKey(e.name)));

  for (const existing of readdirSync(safeDst)) {
    if (!srcNameKeys.has(normalizeNtfsKey(existing))) {
      removeNtfsWithRetry(join(safeDst, existing));
    }
  }

  for (const entry of srcEntries) {
    if (process.platform === "win32" && isNtfsReservedName(entry.name)) continue;
    const from = join(safeSrc, entry.name);
    const to = join(safeDst, entry.name);
    if (entry.isDirectory()) {
      mirrorRealDirNtfs(from, to);
    } else {
      if (isReparseOrSymlink(to)) {
        removeNtfsWithRetry(to);
      }
      runNtfsWithRetry(() => {
        cpSync(from, to, { force: true, dereference: true });
      });
    }
  }
}

function isTransientNtfsError(err: unknown): boolean {
  const code = (err as { code?: string } | null)?.code ?? "";
  return code === "EBUSY" || code === "EPERM" || code === "EACCES" || code === "EMFILE";
}

export function runNtfsWithRetry<T>(fn: () => T, attempts = 6): T {
  let lastErr: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      return fn();
    } catch (err) {
      lastErr = err;
      if (!isTransientNtfsError(err) || i === attempts - 1) throw err;
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 25 * (i + 1));
    }
  }
  throw lastErr;
}

export function removeNtfsWithRetry(targetPath: string): void {
  const safe = toNtfsSafePath(targetPath);
  if (!existsSync(safe) && !isReparseOrSymlink(safe)) return;
  runNtfsWithRetry(() => {
    rmSync(safe, { recursive: true, force: true });
  });
}

/**
 * NTFS-safe atomic binary promotion:
 * - Handles running Windows `.exe`/`.dll` files via rename-while-running to `.prev`
 * - Falls back to `.prev.<pid>` if `.prev` itself is still locked by an older process
 * - Retries transient EBUSY/EPERM (Windows Defender / indexer locks)
 * - Rolls back automatically if copy fails.
 */
export function atomicReplaceFileNtfs(stagedPath: string, destPath: string): void {
  const safeStaged = toNtfsSafePath(stagedPath);
  const safeDest = toNtfsSafePath(destPath);
  ensureRealDirectory(dirname(safeDest));

  const primaryBackup = `${safeDest}.prev`;
  let activeBackup = primaryBackup;
  let movedExisting = false;

  if (existsSync(safeDest)) {
    try {
      removeNtfsWithRetry(primaryBackup);
    } catch {
      activeBackup = `${safeDest}.prev.${process.pid}`;
    }
    try {
      runNtfsWithRetry(() => renameSync(safeDest, activeBackup));
      movedExisting = true;
    } catch {
      // Fallback: attempt direct copy below
    }
  }

  try {
    runNtfsWithRetry(() => copyFileSync(safeStaged, safeDest));
  } catch (err) {
    if (movedExisting && existsSync(activeBackup) && !existsSync(safeDest)) {
      try {
        runNtfsWithRetry(() => renameSync(activeBackup, safeDest));
      } catch {
        // Best-effort rollback
      }
    }
    throw err;
  }
}

export function scanNtfsJunctions(rootDir: string): string[] {
  const found: string[] = [];
  if (!existsSync(rootDir)) return found;
  const stack = [rootDir];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    if (isReparseOrSymlink(cur)) {
      found.push(cur);
      continue;
    }
    for (const entry of readdirSync(toNtfsSafePath(cur), { withFileTypes: true })) {
      const full = join(cur, entry.name);
      if (isReparseOrSymlink(full)) {
        found.push(full);
      } else if (entry.isDirectory()) {
        stack.push(full);
      }
    }
  }
  return found;
}
