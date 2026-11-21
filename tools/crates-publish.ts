#!/usr/bin/env bun
// Publishes the fw crates to crates.io. The roots are product.toml `crates_io_publish_order`; the set published is their
// closure over normal and build path dependencies inside the workspace, read from `cargo metadata`, so a crate a root
// depends on can never be left out. One `cargo publish` call takes the whole set and orders it itself.
//   bun tools/crates-publish.ts --dry-run     package and verify everything, upload nothing
//   bun tools/crates-publish.ts               publish every crate whose version is not on crates.io yet
// Other arguments go to `cargo publish` unchanged (for example --allow-dirty on a scratch tree).
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const product = Bun.TOML.parse(await Bun.file(join(root, "product.toml")).text()) as { crates_io_publish_order: string[] };
const passthrough = Bun.argv.slice(2);
const dryRun = passthrough.includes("--dry-run");

type Dependency = { name: string; kind: string | null; path?: string; source?: string };
type Package = { id: string; name: string; version: string; publish: string[] | null; dependencies: Dependency[] };
const metadata = JSON.parse(Bun.spawnSync(["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"], { cwd: root, stderr: "inherit" }).stdout.toString()) as { packages: Package[] };
const members = new Map(metadata.packages.map((p) => [p.name, p]));

const closure = new Set<string>();
const problems: string[] = [];
const visit = (name: string) => {
  if (closure.has(name)) return;
  const pkg = members.get(name);
  if (!pkg) return void problems.push(`${name}: not a workspace member`);
  closure.add(name);
  if (pkg.publish?.length === 0) problems.push(`${name}: publish = false, but a published crate depends on it`);
  for (const dependency of pkg.dependencies) {
    if (dependency.kind === "dev") continue;
    if (dependency.source?.startsWith("git+")) problems.push(`${name}: git dependency ${dependency.name} cannot go to crates.io`);
    if (dependency.path) visit(dependency.name);
  }
};
product.crates_io_publish_order.forEach(visit);
if (problems.length) {
  console.error(problems.join("\n"));
  process.exit(1);
}

// The sparse index answers which versions exist, so a rerun after a partial publish uploads only what is missing.
const indexPath = (name: string) => (name.length <= 2 ? `${name.length}/${name}` : name.length === 3 ? `3/${name[0]}/${name}` : `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`);
const published = async (pkg: Package) => {
  const response = await fetch(`https://index.crates.io/${indexPath(pkg.name.toLowerCase())}`, { headers: { "user-agent": "fw-release (github.com/aphrody-labs/fw)" } });
  if (response.status === 404) return false;
  if (!response.ok) throw new Error(`index.crates.io ${pkg.name}: ${response.status}`);
  return (await response.text()).split("\n").some((line) => line && JSON.parse(line).vers === pkg.version);
};
const pending: string[] = [];
for (const name of closure) if (!(await published(members.get(name)!))) pending.push(name);
console.log(JSON.stringify({ closure: [...closure], pending, dryRun }));
if (pending.length === 0) process.exit(0);

const publish = Bun.spawnSync(["cargo", "publish", "--locked", ...passthrough, ...pending.flatMap((name) => ["-p", name])], { cwd: root, stdout: "inherit", stderr: "inherit" });
process.exit(publish.exitCode ?? 1);
