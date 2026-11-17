# browser-index

HNSW approximate nearest-neighbour search: scalar and SIMD distance kernels (AVX2 with a portable
fallback), symmetric int8 quantisation for graph traversal, optional exact f32 rescoring, upserts and
tombstone deletes, parallel batch construction and versioned, checksummed, memory-mapped persistence.

```rust
use browser_index::{Index, IndexParams};

let index = Index::new(IndexParams::new(3)).unwrap();
index.add(7, &[1.0, 0.0, 0.0]).unwrap();
let hits = index.search(&[0.9, 0.1, 0.0], 1, None).unwrap();
assert_eq!(hits[0].0, 7);
```

`examples/index_bench.rs` builds a deterministic clustered corpus (`synth`) and reports build time, recall
and latency. The persistence format keeps its historical `BXCIDX01` magic and version so existing index
files stay readable.

Legacy published name: `bun_browser_index` (crates.io, 1.4.8). This crate is the restored source under the
repository's own name; it is not published.

## Test

```sh
cargo test -p browser-index
cargo clippy -p browser-index --all-targets -- -D warnings
```
