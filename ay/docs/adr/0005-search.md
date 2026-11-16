# ADR-5: Search assembled from upstream Rust components

Status: proposed, 2026-11-16; accepted after the index benchmark.

## Decision (proposed)
A Rust crawler, a Rust full-text and vector index (tantivy is the first candidate, benchmarked against alternatives
on the same corpus), a ranker, and a Bun front end replicating google.com. The AI mode answers through the
OpenAI-compatible model API. Image search and file Q&A share the index.

## Scope
The index starts from seeded domains and grows; size and freshness are measured rows in the feature matrix.
