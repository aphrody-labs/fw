// Manifest and per-hash entries from R2, held in the isolate for a short time.
import type { ImageEntry, Manifest, VideoEntry } from "../../../tools/media-cdn/manifest.ts";

const TTL_MS = 30_000;
let current: { at: number; manifest: Manifest } | undefined;
const entries = new Map<string, unknown>();

export async function loadManifest(bucket: R2Bucket, now = Date.now()): Promise<Manifest | undefined> {
  if (current && now - current.at < TTL_MS) return current.manifest;
  const object = await bucket.get("v1/manifest.json");
  if (!object) return undefined;
  const manifest = (await object.json()) as Manifest;
  current = { at: now, manifest };
  return manifest;
}

export function resetStore() {
  current = undefined;
  entries.clear();
}

/** Entries of a pinned hash are immutable, so they are kept for the life of the isolate. */
async function pinned<T>(bucket: R2Bucket, key: string): Promise<T | undefined> {
  const hit = entries.get(key);
  if (hit) return hit as T;
  const object = await bucket.get(key);
  if (!object) return undefined;
  const value = (await object.json()) as T;
  entries.set(key, value);
  return value;
}

export async function imageEntry(bucket: R2Bucket, name: string, pin: string | undefined): Promise<{ entry: ImageEntry; sha: string } | undefined> {
  const manifest = await loadManifest(bucket);
  const live = manifest?.images[name];
  if (live && (!pin || live.sha === pin)) return { entry: live, sha: live.sha };
  if (!pin) return undefined;
  const entry = await pinned<ImageEntry>(bucket, `v1/img/${name}.${pin}/entry.json`);
  return entry && { entry, sha: pin };
}

export async function videoEntry(bucket: R2Bucket, name: string, pin: string | undefined): Promise<{ entry: VideoEntry; sha: string } | undefined> {
  const manifest = await loadManifest(bucket);
  const live = manifest?.video[name];
  if (live && (!pin || live.sha === pin)) return { entry: live, sha: live.sha };
  if (!pin) return undefined;
  const entry = await pinned<VideoEntry>(bucket, `v1/video/${name}.${pin}/entry.json`);
  return entry && { entry, sha: pin };
}

export async function spriteSha(bucket: R2Bucket, name: string, pin: string | undefined): Promise<string | undefined> {
  if (pin) return pin;
  return (await loadManifest(bucket))?.sprites[name]?.sha;
}
