// The manifest written by build.ts and read by the Worker: logical names to content hashes, and the rungs that exist.
import { type Codec, SOURCE_ORDER, VIDEO_BREAKPOINTS, VIDEO_MIME, VIDEO_EXTENSION, videoFile, videoKey } from "./grammar.ts";

export interface ImageEntry {
  /** First 8 hex digits of the SHA-256 of the source file. */
  sha: string;
  /** File extension of the stored source (`v1/src/img/<name>.<sha>.<ext>`). */
  source: string;
  width: number;
  height: number;
  alpha: boolean;
  /** Format served to clients that decode neither AVIF nor WebP. */
  fallback: "jpg" | "png";
  /** Rungs built ahead of time, per kind. */
  widths: number[];
  squares: number[];
  /** Encoded size in bytes of every stored rung: `w600.avif`. */
  bytes: Record<string, number>;
}

export interface VideoFile {
  width: number;
  density: number;
  codec: Codec;
  /** Pixel size of the encoded frame. */
  pixels: { width: number; height: number };
  bytes: number;
  /** RFC 6381 codec string, read from the encoded stream. */
  codecs: string;
  bitrate: number;
}

export interface VideoEntry {
  sha: string;
  /** Duration in seconds. */
  duration: number;
  fps: number;
  /** Name of the poster image in `images` when there is one. */
  poster?: string;
  files: VideoFile[];
}

export interface SpriteEntry {
  sha: string;
  symbols: string[];
  bytes: number;
}

export interface Manifest {
  schema: 1;
  /** First 8 hex digits of the SHA-256 of the manifest content without this field. */
  revision: string;
  images: Record<string, ImageEntry>;
  video: Record<string, VideoEntry>;
  sprites: Record<string, SpriteEntry>;
}

export interface Source {
  src: string;
  type: string;
  media?: string;
}

/** The `<source>` rows of a video in selection order: for each breakpoint, AV1, VP9 and H.264 (codec-qualified types, which googlebook.google omits). */
export function sourcesOf(name: string, entry: VideoEntry, origin = ""): Source[] {
  const out: Source[] = [];
  for (const point of VIDEO_BREAKPOINTS) {
    for (const row of SOURCE_ORDER) {
      const file = entry.files.find((f) => f.width === point.width && f.density === row.density && f.codec === row.codec);
      if (!file) continue;
      const source: Source = {
        src: `${origin}/${videoKey(name, entry.sha, videoFile(file.width, file.density, file.codec))}`,
        type: `${VIDEO_MIME[VIDEO_EXTENSION[file.codec]]}; codecs="${file.codecs}"`,
      };
      if (point.media) source.media = point.media;
      out.push(source);
    }
  }
  return out;
}

