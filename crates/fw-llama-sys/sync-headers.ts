// Refresh the vendored llama.cpp/ggml headers in include/ from one upstream tag and record it in
// include/LLAMA_CPP_TAG. Usage: bun sync-headers.ts [tag]   (default: latest ggml-org/llama.cpp release)
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

const REPO = "ggml-org/llama.cpp";
const HEADERS: Record<string, string> = {
  "llama.h": "include/llama.h",
  "ggml.h": "ggml/include/ggml.h",
  "gguf.h": "ggml/include/gguf.h",
  "ggml-alloc.h": "ggml/include/ggml-alloc.h",
  "ggml-backend.h": "ggml/include/ggml-backend.h",
  "ggml-cpu.h": "ggml/include/ggml-cpu.h",
  "ggml-opt.h": "ggml/include/ggml-opt.h",
  "mtmd.h": "tools/mtmd/mtmd.h",
  "mtmd-helper.h": "tools/mtmd/mtmd-helper.h",
};

const gh = async (path: string, accept = "application/vnd.github+json") => {
  const proc = Bun.spawn(["gh", "api", "-H", `Accept: ${accept}`, path], { stdout: "pipe", stderr: "inherit" });
  const out = await new Response(proc.stdout).arrayBuffer();
  if ((await proc.exited) !== 0) throw new Error(`gh api ${path} failed`);
  return new Uint8Array(out);
};

const tag = process.argv[2] ?? JSON.parse(new TextDecoder().decode(await gh(`repos/${REPO}/releases/latest`))).tag_name;
const dir = join(import.meta.dir, "include");
await mkdir(dir, { recursive: true });
await Promise.all(
  Object.entries(HEADERS).map(async ([name, path]) =>
    writeFile(join(dir, name), await gh(`repos/${REPO}/contents/${path}?ref=${tag}`, "application/vnd.github.raw+json")),
  ),
);
await writeFile(join(dir, "LLAMA_CPP_TAG"), `${tag}\n`);
console.log(`synced ${Object.keys(HEADERS).length} headers from ${REPO}@${tag}`);
