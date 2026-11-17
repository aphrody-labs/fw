// bun:ffi binding to libarchive.so.13: reading, extracting and creating archives (tar, tar.gz, zip, ...).
//
// The library is loaded by its soname; set AY_LIBARCHIVE to a path or another soname to override it.
//
// Bound functions:
//   archive_read_new / archive_read_support_filter_all / archive_read_support_format_all
//   archive_read_open_filename / archive_read_next_header / archive_read_data_skip
//   archive_read_data / archive_read_free
//   archive_entry_pathname / archive_entry_size / archive_entry_filetype
//   archive_write_disk_new / archive_write_disk_set_options / archive_write_header
//   archive_write_data / archive_write_free
//   archive_write_new / archive_write_add_filter_gzip / archive_write_set_format_pax_restricted
//   archive_write_open_filename / archive_write_close
//   archive_entry_new / archive_entry_set_pathname / archive_entry_set_size
//   archive_entry_set_filetype / archive_entry_set_perm / archive_entry_free

import { readFileSync } from "node:fs";
import { dlopen, FFIType, ptr, type Pointer } from "bun:ffi";

// File type constants (POSIX mode bits)
const AE_IFREG = 0o100000; // regular file
const AE_IFDIR = 0o040000; // directory

// libarchive return codes
const ARCHIVE_OK = 0;
const ARCHIVE_EOF = 1;

// archive_write_disk extraction flags
const ARCHIVE_EXTRACT_PERM = 0x0002;
const ARCHIVE_EXTRACT_TIME = 0x0004;
const ARCHIVE_EXTRACT_ACL = 0x0020;
const ARCHIVE_EXTRACT_FFLAGS = 0x0040;
// Refuse entries that traverse upward with ".." or write through symlinks (path traversal).
const ARCHIVE_EXTRACT_SECURE_SYMLINKS = 0x0100;
const ARCHIVE_EXTRACT_SECURE_NODOTDOT = 0x0200;

const lib = dlopen(process.env.AY_LIBARCHIVE ?? "libarchive.so.13", {
  // ── Read ──────────────────────────────────────────────────────────────────
  archive_read_new: { args: [], returns: FFIType.ptr },
  archive_read_support_filter_all: { args: [FFIType.ptr], returns: FFIType.i32 },
  archive_read_support_format_all: { args: [FFIType.ptr], returns: FFIType.i32 },
  archive_read_open_filename: {
    args: [FFIType.ptr, FFIType.cstring, FFIType.u64],
    returns: FFIType.i32,
  },
  archive_read_next_header: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  archive_read_data_skip: { args: [FFIType.ptr], returns: FFIType.i32 },
  archive_read_data: { args: [FFIType.ptr, FFIType.ptr, FFIType.u64], returns: FFIType.i64 },
  archive_read_free: { args: [FFIType.ptr], returns: FFIType.i32 },

  // ── Entry accessors ───────────────────────────────────────────────────────
  archive_entry_pathname: { args: [FFIType.ptr], returns: FFIType.cstring },
  archive_entry_size: { args: [FFIType.ptr], returns: FFIType.i64 },
  archive_entry_filetype: { args: [FFIType.ptr], returns: FFIType.u32 },

  // ── Write to disk (extract) ───────────────────────────────────────────────
  archive_write_disk_new: { args: [], returns: FFIType.ptr },
  archive_write_disk_set_options: { args: [FFIType.ptr, FFIType.i32], returns: FFIType.i32 },
  archive_write_header: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  archive_write_data: { args: [FFIType.ptr, FFIType.ptr, FFIType.u64], returns: FFIType.i64 },
  archive_write_free: { args: [FFIType.ptr], returns: FFIType.i32 },

  // ── Write archive (creation) ──────────────────────────────────────────────
  archive_write_new: { args: [], returns: FFIType.ptr },
  archive_write_add_filter_gzip: { args: [FFIType.ptr], returns: FFIType.i32 },
  archive_write_set_format_pax_restricted: { args: [FFIType.ptr], returns: FFIType.i32 },
  archive_write_open_filename: { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.i32 },
  archive_write_close: { args: [FFIType.ptr], returns: FFIType.i32 },

  // ── Entry new / set / free ────────────────────────────────────────────────
  archive_entry_new: { args: [], returns: FFIType.ptr },
  archive_entry_set_pathname: { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.void },
  archive_entry_set_size: { args: [FFIType.ptr, FFIType.i64], returns: FFIType.void },
  archive_entry_set_filetype: { args: [FFIType.ptr, FFIType.u32], returns: FFIType.void },
  archive_entry_set_perm: { args: [FFIType.ptr, FFIType.i32], returns: FFIType.void },
  archive_entry_free: { args: [FFIType.ptr], returns: FFIType.void },
});

const s = lib.symbols;

/** Encodes a string as a C string (NUL-terminated UTF-8 in a Buffer). */
function cstr(str: string): Buffer {
  return Buffer.from(str + "\0", "utf8");
}

export interface ArchiveEntry {
  pathname: string;
  size: number;
  isDirectory: boolean;
}

/**
 * Lists the entries of an archive (tar, tar.gz, zip, ...).
 * Returns an array of `{ pathname, size, isDirectory }` objects.
 */
export function listEntries(path: string): ArchiveEntry[] {
  const a = s.archive_read_new();
  if (!a) throw new Error("archive_read_new failed");

  s.archive_read_support_filter_all(a);
  s.archive_read_support_format_all(a);

  const pathBuf = cstr(path);
  const rc = s.archive_read_open_filename(a, ptr(pathBuf), 10240n);
  if (rc !== ARCHIVE_OK) {
    s.archive_read_free(a);
    throw new Error(`Cannot open archive: ${path} (code ${rc})`);
  }

  const entries: ArchiveEntry[] = [];
  // Buffer for archive_entry**: pointer to pointer (8 bytes on x86_64)
  const entryPtrBuf = Buffer.alloc(8);

  for (;;) {
    const r = s.archive_read_next_header(a, ptr(entryPtrBuf));
    if (r === ARCHIVE_EOF) break;
    if (r !== ARCHIVE_OK) {
      s.archive_read_free(a);
      throw new Error(`archive_read_next_header failed (code ${r})`);
    }

    // Read the archive_entry* pointer out of entryPtrBuf (little-endian x86_64)
    const entryPtr = Number(entryPtrBuf.readBigUInt64LE(0)) as Pointer;

    const pathnameRaw = s.archive_entry_pathname(entryPtr);
    const pathname = pathnameRaw?.toString() ?? "";
    const size = Number(s.archive_entry_size(entryPtr));
    const filetype = Number(s.archive_entry_filetype(entryPtr));
    const isDirectory = (filetype & 0xf000) === AE_IFDIR;

    entries.push({ pathname, size, isDirectory });
    s.archive_read_data_skip(a);
  }

  s.archive_read_free(a);
  return entries;
}

/**
 * Extracts every entry of an archive into `destDir`.
 * Entries that would escape `destDir` (`..` components, symlink traversal) are skipped.
 */
export function extractAll(archivePath: string, destDir: string): void {
  const a = s.archive_read_new();
  if (!a) throw new Error("archive_read_new failed");

  s.archive_read_support_filter_all(a);
  s.archive_read_support_format_all(a);

  const pathBuf = cstr(archivePath);
  const rc = s.archive_read_open_filename(a, ptr(pathBuf), 10240n);
  if (rc !== ARCHIVE_OK) {
    s.archive_read_free(a);
    throw new Error(`Cannot open archive: ${archivePath} (code ${rc})`);
  }

  const disk = s.archive_write_disk_new();
  if (!disk) {
    s.archive_read_free(a);
    throw new Error("archive_write_disk_new failed");
  }

  const flags =
    ARCHIVE_EXTRACT_PERM |
    ARCHIVE_EXTRACT_TIME |
    ARCHIVE_EXTRACT_ACL |
    ARCHIVE_EXTRACT_FFLAGS |
    ARCHIVE_EXTRACT_SECURE_SYMLINKS |
    ARCHIVE_EXTRACT_SECURE_NODOTDOT;
  s.archive_write_disk_set_options(disk, flags);

  const entryPtrBuf = Buffer.alloc(8);
  const chunkSize = 65536;
  const chunk = new Uint8Array(chunkSize);

  for (;;) {
    const r = s.archive_read_next_header(a, ptr(entryPtrBuf));
    if (r === ARCHIVE_EOF) break;
    if (r !== ARCHIVE_OK) {
      s.archive_write_free(disk);
      s.archive_read_free(a);
      throw new Error(`archive_read_next_header failed (code ${r})`);
    }

    const entryPtr = Number(entryPtrBuf.readBigUInt64LE(0)) as Pointer;

    // Prefix the pathname with destDir
    const pathnameRaw = s.archive_entry_pathname(entryPtr);
    const originalPathname = pathnameRaw?.toString() ?? "";
    // Strip leading slashes to avoid absolute paths
    const normalizedPathname = originalPathname.replace(/^\/+/, "");
    const destPath = `${destDir}/${normalizedPathname}`;
    const destBuf = cstr(destPath);
    s.archive_entry_set_pathname(entryPtr, ptr(destBuf));

    const wrc = s.archive_write_header(disk, entryPtr);

    // Restore the original pathname
    const origBuf = cstr(originalPathname);
    s.archive_entry_set_pathname(entryPtr, ptr(origBuf));

    if (wrc !== ARCHIVE_OK) continue;

    // Copy the data as a stream
    for (;;) {
      const nread = Number(s.archive_read_data(a, ptr(chunk), BigInt(chunkSize)));
      if (nread === 0) break;
      if (nread < 0) {
        s.archive_write_free(disk);
        s.archive_read_free(a);
        throw new Error(`archive_read_data failed (code ${nread})`);
      }
      s.archive_write_data(disk, ptr(chunk), BigInt(nread));
    }
  }

  s.archive_write_free(disk);
  s.archive_read_free(a);
}

/**
 * Creates a tar.gz archive from an array of files.
 *
 * @param outputPath - Output path of the archive.
 * @param files      - List of `{ path, name }`: `path` = path on disk,
 *                     `name` = name of the entry inside the archive.
 */
export function createTarGz(
  outputPath: string,
  files: { path: string; name: string }[],
): void {
  const a = s.archive_write_new();
  if (!a) throw new Error("archive_write_new failed");

  s.archive_write_add_filter_gzip(a);
  s.archive_write_set_format_pax_restricted(a);

  const outBuf = cstr(outputPath);
  const rc = s.archive_write_open_filename(a, ptr(outBuf));
  if (rc !== ARCHIVE_OK) {
    s.archive_write_free(a);
    throw new Error(`Cannot create archive: ${outputPath} (code ${rc})`);
  }

  for (const { path: filePath, name } of files) {
    const data = new Uint8Array(readFileSync(filePath));

    const entry = s.archive_entry_new();
    if (!entry) continue;

    const nameBuf = cstr(name);
    s.archive_entry_set_pathname(entry, ptr(nameBuf));
    s.archive_entry_set_size(entry, BigInt(data.byteLength));
    s.archive_entry_set_filetype(entry, AE_IFREG);
    s.archive_entry_set_perm(entry, 0o644);

    const wrc = s.archive_write_header(a, entry);
    if (wrc === ARCHIVE_OK && data.byteLength > 0) {
      s.archive_write_data(a, ptr(data), BigInt(data.byteLength));
    }
    s.archive_entry_free(entry);
  }

  s.archive_write_close(a);
  s.archive_write_free(a);
}
