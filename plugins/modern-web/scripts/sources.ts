// Upstreams the sync pulls. Paths are git sparse-checkout patterns (no-cone); refs default to the remote HEAD.
// Spec selection is not listed here: it is derived from w3c/webref (see selectSpecs in sync.ts).

export type Domain = "bun" | "mwg" | "webgpu" | "wasm" | "specs";

export interface RepoSource {
  repo: string;
  sparse: string[];
  role: string;
}

export const repos = {
  bun: {
    repo: "oven-sh/bun",
    sparse: ["/docs/**", "/packages/bun-types/**"],
    role: "Bun upstream: docs, blog and release notes (docs/blog), runtime API types",
  },
  bunFork: {
    repo: "aphrody-labs/bun",
    sparse: ["/docs/**", "/packages/bun-types/**"],
    role: "Aphrody runtime fork: pages and API types that differ from upstream",
  },
  mwg: {
    repo: "GoogleChrome/modern-web-guidance-src",
    sparse: ["/guides/**", "/skills-src/**", "/package.json"],
    role: "Modern Web Guidance guides and standalone skills",
  },
  mwgFork: {
    repo: "aphrody-labs/modern-web-guidance-src",
    sparse: ["/package.json", "/scripts/**"],
    role: "Bun-migrated fork of modern-web-guidance-src",
  },
  gpuweb: {
    repo: "gpuweb/gpuweb",
    sparse: ["/spec/index.bs", "/wgsl/index.bs"],
    role: "WebGPU and WGSL Bikeshed sources",
  },
  wgpu: {
    repo: "gfx-rs/wgpu",
    sparse: [
      "/CHANGELOG.md",
      "/README.md",
      "/docs/**",
      "/naga/README.md",
      "/wgpu/src/api/**",
      "/wgpu-types/src/**",
    ],
    role: "wgpu and Naga: changelog, docs, public API doc comments",
  },
  wasmSpec: {
    repo: "WebAssembly/spec",
    sparse: [
      "/document/core/**",
      "/document/js-api/index.bs",
      "/document/web-api/index.bs",
    ],
    role: "WebAssembly core (RST), JS API and Web API (Bikeshed) sources",
  },
  wasi: {
    repo: "WebAssembly/WASI",
    sparse: ["/README.md", "/proposals/**", "/docs/**"],
    role: "WASI proposals and WIT interfaces",
  },
  wasmBindgen: {
    repo: "wasm-bindgen/wasm-bindgen",
    sparse: ["/guide/src/**", "/CHANGELOG.md"],
    role: "wasm-bindgen guide (mdBook) and changelog",
  },
  webref: {
    repo: "w3c/webref",
    sparse: ["/ed/index.json"],
    role: "W3C webref crawl: spec metadata, headings and definitions",
  },
} satisfies Record<string, RepoSource>;

export type RepoKey = keyof typeof repos;

// Spec repositories whose sections belong to a dedicated domain rather than the generic web-specs skill.
export const domainBySpecRepo: Record<string, Domain> = {
  "https://github.com/gpuweb/gpuweb": "webgpu",
  "https://github.com/WebAssembly/spec": "wasm",
};

export const releaseFeeds = [
  { repo: "oven-sh/bun", domain: "bun" as Domain, limit: 400 },
  { repo: "aphrody-labs/bun", domain: "bun" as Domain, limit: 60 },
  { repo: "gfx-rs/wgpu", domain: "webgpu" as Domain, limit: 40 },
  { repo: "wasm-bindgen/wasm-bindgen", domain: "wasm" as Domain, limit: 40 },
];

export const npmPackages = ["modern-web-guidance", "web-features"];
