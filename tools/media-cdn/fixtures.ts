#!/usr/bin/env bun
// media-cdn: generates the test sources that build.ts consumes (our own synthetic content, nothing downloaded).
//   bun tools/media-cdn/fixtures.ts --out DIR [--icons DIR]
// img/test-photo.png (4096x2304, opaque), img/test-icon.png (512x512 with alpha), video/test-wide.mp4 (3840x2160, 30 fps, 4 s),
// video/test-square.mp4 (2400x2400, 2 s: the 1440w 2x rung must be absent), sprite/ui/<icon>.svg from the Aphrody icon set.
import { copyFile, mkdir, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
};
const out = resolve(opt("out") ?? (console.error("--out DIR is required"), process.exit(2)));
const icons = resolve(opt("icons") ?? join(import.meta.dir, "../../aphrody/assets/icons"));

async function run(cmd: string[]) {
  const proc = Bun.spawn(cmd, { stdout: "ignore", stderr: "pipe" });
  const [stderr, code] = await Promise.all([new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0) throw new Error(`${cmd.join(" ")}\n${stderr.split("\n").slice(-5).join("\n")}`);
}
const ffmpeg = (...rest: string[]) => run(["ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y", ...rest]);

for (const dir of ["img", "video", "sprite/ui"]) await mkdir(join(out, dir), { recursive: true });

await ffmpeg("-f", "lavfi", "-i", "mandelbrot=s=4096x2304", "-frames:v", "1", join(out, "img/test-photo.png"));
await run(["magick", "-size", "512x512", "xc:none", "-fill", "#e2dbad", "-draw", "circle 256,256 256,24", "-fill", "#1c1b18", "-draw", "circle 256,256 256,160", `png:${join(out, "img/test-icon.png")}`]);
await ffmpeg("-f", "lavfi", "-i", "gradients=s=3840x2160:r=30:d=4:speed=0.03:n=5", "-c:v", "libx264", "-preset", "veryfast", "-crf", "14", "-pix_fmt", "yuv420p", join(out, "video/test-wide.mp4"));
await ffmpeg("-f", "lavfi", "-i", "gradients=s=2400x2400:r=30:d=2:speed=0.05:n=4", "-c:v", "libx264", "-preset", "veryfast", "-crf", "14", "-pix_fmt", "yuv420p", join(out, "video/test-square.mp4"));

let copied = 0;
for (const entry of await readdir(icons, { withFileTypes: true })) {
  if (!entry.isFile() || !entry.name.endsWith(".svg")) continue;
  await copyFile(join(icons, entry.name), join(out, "sprite/ui", entry.name.toLowerCase()));
  copied++;
}
console.log(`fixtures in ${out}: 2 images, 2 videos, ${copied} sprite symbols`);
