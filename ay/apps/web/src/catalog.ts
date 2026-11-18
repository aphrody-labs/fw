import { join, resolve } from "node:path";

export interface FeatureSummary {
  readonly product: string;
  readonly id: string;
  readonly title: string;
  readonly layer: string;
  readonly status: string;
}

export interface StudioEntry {
  readonly id: string;
  readonly title: string;
  readonly host: string;
  readonly summary: string;
}

export interface PortalSnapshot {
  readonly product: { name: string; display: string; cli: string; repo: string; owner: string };
  readonly orgs: Record<string, string>;
  readonly products: ReadonlyArray<{ key: string; title: string; owner: string; keywords: readonly string[] }>;
  readonly repos: ReadonlyArray<{ key: string; slug: string; kind: string }>;
  readonly registries: { npm: string; ownCrates: string; legacyCratesPrefix: string; legacyNpm: readonly string[]; legacyCratesCount: number };
  readonly sdk: { languages: readonly string[]; arch: readonly string[]; os: readonly string[]; gpu: readonly string[] };
  readonly fleetRoles: readonly string[];
  readonly featureStats: { total: number; done: number; doing: number; todo: number; byProduct: Record<string, { total: number; done: number }> };
  readonly features: readonly FeatureSummary[];
  readonly abi: { version: number; name: string; specSha256: string; constantsCount: number; functions: readonly { name: string; returns: string; doc?: string }[] };
  readonly studios: readonly StudioEntry[];
  readonly r2Buckets: ReadonlyArray<{ host: string; bucket: string }>;
}

export const STUDIOS: readonly StudioEntry[] = [
  { id: "film", title: "FilmCraft", host: "filmcraft.aphrody.app", summary: "Video composition, decode, frame graph, and remux studio" },
  { id: "art", title: "ArtCraft", host: "artcraft.aphrody.app", summary: "Vector/raster generation and shader-driven art canvas" },
  { id: "photo", title: "PhotoCraft", host: "photocraft.aphrody.app", summary: "GPU image editing and color grading workspace" },
  { id: "vector", title: "VectorCraft", host: "vectorcraft.aphrody.app", summary: "Bezier, SVG, and analytical SDF authoring" },
  { id: "effect", title: "EffectCraft", host: "effectcraft.aphrody.app", summary: "Node-based WebGPU compositing and effects" },
  { id: "light", title: "LightCraft", host: "lightcraft.aphrody.app", summary: "RAW processing, tone mapping, and lens pipeline" },
  { id: "pdf", title: "PdfCraft", host: "pdfcraft.aphrody.app", summary: "Native PDF inspection, layout, and rendering" },
  { id: "word", title: "WordCraft", host: "wordcraft.aphrody.app", summary: "Technical authoring, Markdown/MDX, and typography" },
  { id: "sound", title: "SoundCraft", host: "soundcraft.aphrody.app", summary: "Low-latency audio graph, synthesis, and mastering" },
  { id: "design", title: "DesignCraft", host: "designcraft.aphrody.app", summary: "Material 3 Expressive tokens, DTCG variables, and Code Connect" },
  { id: "cad", title: "CadCraft", host: "cadcraft.aphrody.app", summary: "Parametric 3D solids and engineering constraints" },
  { id: "deck", title: "DeckCraft", host: "deckcraft.aphrody.app", summary: "Presentation slides and live interactive demos" },
  { id: "grid", title: "GridCraft", host: "gridcraft.aphrody.app", summary: "High-density data grids and formula computation" },
  { id: "fig", title: "FigCraft", host: "figcraft.aphrody.app", summary: "Interface prototyping and layout constraints" },
] as const;

export const STUDIO_HOST_MAP: Readonly<Record<string, string>> = {
  "filmcraft.aphrody.app": "film",
  "app-video.aphrody.app": "film",
  "artcraft.aphrody.app": "art",
  "app-generation.aphrody.app": "art",
  "art.aphrody.com": "art",
  "photocraft.aphrody.app": "photo",
  "app-images.aphrody.app": "photo",
  "photos.aphrody.com": "photo",
  "vectorcraft.aphrody.app": "vector",
  "app-vectors.aphrody.app": "vector",
  "effectcraft.aphrody.app": "effect",
  "app-compositing.aphrody.app": "effect",
  "lightcraft.aphrody.app": "light",
  "app-photo-processing.aphrody.app": "light",
  "pdfcraft.aphrody.app": "pdf",
  "app-pdf.aphrody.app": "pdf",
  "pdf.aphrody.com": "pdf",
  "wordcraft.aphrody.app": "word",
  "app-documents.aphrody.app": "word",
  "soundcraft.aphrody.app": "sound",
  "app-audio.aphrody.app": "sound",
  "sound.aphrody.com": "sound",
  "designcraft.aphrody.app": "design",
  "app-layout.aphrody.app": "design",
  "design.aphrody.com": "design",
  "cadcraft.aphrody.app": "cad",
  "app-cad.aphrody.app": "cad",
  "cad.aphrody.com": "cad",
  "deckcraft.aphrody.app": "deck",
  "app-slides.aphrody.app": "deck",
  "slides.aphrody.com": "deck",
  "gridcraft.aphrody.app": "grid",
  "app-tables.aphrody.app": "grid",
  "sheets.aphrody.com": "grid",
  "figcraft.aphrody.app": "fig",
  "app-interface-layouts.aphrody.app": "fig",
  "fig.aphrody.com": "fig",
};

export const R2_BUCKETS: ReadonlyArray<{ host: string; bucket: string }> = [
  { host: "assets.aphrody.app", bucket: "aphrody-assets" },
  { host: "dist.aphrody.app", bucket: "aphrody-dist" },
  { host: "video.aphrody.app", bucket: "aphrody-video" },
  { host: "bun-lib.aphrody.app", bucket: "bun-lib" },
] as const;

export const DEFAULT_PORTAL_SNAPSHOT: PortalSnapshot = {
  product: { name: "aphrody", display: "Aphrody", cli: "ay", repo: "ay", owner: "aphrody-labs" },
  orgs: { labs: "aphrody-labs" },
  products: [
    { key: "os", title: "Aphrody OS", owner: "aphrody", keywords: ["os", "kernel", "linux", "wayland", "shell"] },
    { key: "browser", title: "Aphrody Browser", owner: "browser", keywords: ["browser", "chromium", "cef", "webmcp"] },
    { key: "search", title: "Aphrody Search", owner: "aphrody", keywords: ["search", "crawl", "index", "rag", "ai"] },
    { key: "cloud", title: "Aphrody Cloud", owner: "aphrody", keywords: ["cloud", "edge", "workers", "r2", "bun"] },
    { key: "craft", title: "Craft Anything", owner: "craft-anything", keywords: ["craft", "canvas", "video", "shader"] },
    { key: "brand", title: "Aphrody brand", owner: "ay", keywords: ["brand", "m3", "tokens", "icons"] },
  ],
  repos: [],
  registries: { npm: "@aphrody", ownCrates: "ay-", legacyCratesPrefix: "aphrody-", legacyNpm: ["@aphrody/chromium"], legacyCratesCount: 32 },
  sdk: {
    languages: ["c", "cpp", "objc", "asm", "wat", "rust", "zig", "wgsl", "python", "dotnet", "bun"],
    arch: ["x86_64", "aarch64", "riscv64", "wasm32", "wasm64"],
    os: ["linux", "windows", "macos", "android", "freestanding", "wasi", "browser"],
    gpu: ["webgpu", "vulkan", "d3d12", "metal"],
  },
  fleetRoles: ["build", "test", "prod"],
  featureStats: { total: 0, done: 0, doing: 0, todo: 0, byProduct: {} },
  features: [],
  abi: { version: 1, name: "ay", specSha256: "edge", constantsCount: 22, functions: [] },
  studios: STUDIOS,
  r2Buckets: R2_BUCKETS,
};

export function resolveRepoRoot(fromDir = import.meta.dir): string {
  return resolve(fromDir, "../../..");
}

export async function loadPortalSnapshot(root = resolveRepoRoot()): Promise<PortalSnapshot> {
  const ayFile = Bun.file(join(root, "ay.toml"));
  if (!(await ayFile.exists())) return DEFAULT_PORTAL_SNAPSHOT;
  const ay = Bun.TOML.parse(await ayFile.text()) as Record<string, any>;

  const namingFile = Bun.file(join(root, "naming.toml"));
  const naming = (await namingFile.exists()) ? (Bun.TOML.parse(await namingFile.text()) as Record<string, any>) : {};

  const featFile = Bun.file(join(root, "docs/features.toml"));
  const featRaw = (await featFile.exists())
    ? ((Bun.TOML.parse(await featFile.text()) as { feature?: FeatureSummary[] }).feature ?? [])
    : [];
  const byProduct: Record<string, { total: number; done: number }> = {};
  let done = 0;
  let doing = 0;
  let todo = 0;
  for (const f of featRaw) {
    const slot = (byProduct[f.product] ??= { total: 0, done: 0 });
    slot.total++;
    if (f.status === "done") {
      done++;
      slot.done++;
    } else if (f.status === "doing") {
      doing++;
    } else {
      todo++;
    }
  }

  const abiFile = Bun.file(join(root, "abi/gen/ay_abi.json"));
  const abiJson = (await abiFile.exists()) ? ((await abiFile.json()) as Record<string, any>) : {};

  const products = Object.entries((ay.products ?? {}) as Record<string, { owner: string; title: string; keywords?: string[] }>).map(
    ([key, v]) => ({ key, title: v.title, owner: v.owner, keywords: v.keywords ?? [] }),
  );
  const repos = Object.entries((ay.repos ?? {}) as Record<string, { slug: string; kind: string }>).map(
    ([key, v]) => ({ key, slug: v.slug, kind: v.kind }),
  );

  return {
    product: ay.product ?? DEFAULT_PORTAL_SNAPSHOT.product,
    orgs: ay.orgs ?? DEFAULT_PORTAL_SNAPSHOT.orgs,
    products,
    repos,
    registries: {
      npm: ay.registries?.npm ?? "@aphrody",
      ownCrates: ay.registries?.own_crates ?? "ay-",
      legacyCratesPrefix: ay.registries?.crates ?? "aphrody-",
      legacyNpm: naming.legacy?.npm ?? [],
      legacyCratesCount: (naming.legacy?.crates ?? []).length,
    },
    sdk: ay.sdk ?? DEFAULT_PORTAL_SNAPSHOT.sdk,
    fleetRoles: ["build", "test", "prod"],
    featureStats: { total: featRaw.length, done, doing, todo, byProduct },
    features: featRaw,
    abi: {
      version: Number(abiJson.version ?? 1),
      name: String(abiJson.name ?? "ay"),
      specSha256: String(abiJson.spec_sha256 ?? ""),
      constantsCount: Array.isArray(abiJson.constants) ? abiJson.constants.length : 0,
      functions: Array.isArray(abiJson.functions) ? abiJson.functions : [],
    },
    studios: STUDIOS,
    r2Buckets: R2_BUCKETS,
  };
}
