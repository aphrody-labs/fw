// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 aphrody contributors

//! Unified RAG storage, embedding, vector scan, hybrid search, and agent tools for aphrody.
//!
//! Provides production-grade SQLite FTS5 lexical retrieval combined with dense vector
//! similarity scanning, Reciprocal Rank Fusion (RRF k=60.0), cross-encoder reranking,
//! incremental file indexing, and agent-facing toolcalls (`rag_search`, `rag_context`).

extern crate self as ay_rag;

#[cfg(feature = "rag")]
pub mod config_resolve;
#[cfg(feature = "rag")]
pub mod embedder;
#[cfg(feature = "rag")]
pub mod error;
#[cfg(feature = "rag")]
pub mod home;
#[cfg(feature = "rag")]
pub mod home_cli;
#[cfg(feature = "rag")]
pub mod indexer;
#[cfg(feature = "rag")]
pub mod pipeline;
#[cfg(feature = "rag")]
pub mod postgres;
#[cfg(feature = "rag")]
pub mod redis_cache;
#[cfg(feature = "rag")]
pub mod reranker;
#[cfg(feature = "rag")]
pub mod schema;
#[cfg(feature = "rag")]
pub mod search;
#[cfg(feature = "rag")]
pub mod store;
#[cfg(feature = "rag")]
pub mod tools;

#[cfg(feature = "rag")]
pub use config_resolve::{
    ResolvedEmbedConfig, ResolvedRerankConfig, rerank_available, resolve_embed, resolve_rerank,
};
#[cfg(feature = "embed-native")]
#[cfg(feature = "rag")]
pub use embedder::NativeEmbedder;
#[cfg(feature = "embed-ort")]
#[cfg(feature = "rag")]
pub use embedder::OrtEmbedder;
#[cfg(feature = "rag")]
pub use embedder::{
    Embedder, LlamaServerEmbedder, MATRYOSHKA_512_DIM, OpenAiCompatEmbedder,
    QWEN3_QUERY_INSTRUCTION, StubEmbedder, format_query_or_passage, truncate_and_normalize_mrl,
};
#[cfg(feature = "rag")]
pub use error::{RagError, Result};
#[cfg(feature = "rag")]
pub use home::{HitKind, UnifiedHit, UnifiedOptions, unified_search};
#[cfg(feature = "rag")]
pub use indexer::{IndexOptions, IndexStats, RagIndexer};
#[cfg(feature = "rag")]
pub use postgres::{
    PgWebIngestResult, ShenronSearchParams, ingest_web_document, search_documents,
    search_shenron_documents, stats as pg_stats,
};
#[cfg(feature = "rag")]
pub use redis_cache::RedisRagCache;
#[cfg(feature = "embed-native")]
#[cfg(feature = "rag")]
pub use reranker::NativeReranker;
#[cfg(feature = "rag")]
pub use reranker::{LlamaServerReranker, NoopReranker, Reranker};
#[cfg(feature = "rag")]
pub use search::{RagSearcher, SearchMode, SearchOptions, SearchResult};
#[cfg(feature = "rag")]
pub use store::{ChunkRecord, DocRecord, RagStats, RagStore, ScoredChunk};
#[cfg(feature = "rag")]
pub use tools::{
    RagContextTool, RagSearchTool, WebIngestTool, register_rag_tools, register_web_ingest_tool,
    scrape_web_markdown,
};

#[cfg(feature = "sql")]
pub mod sql;
#[cfg(feature = "localstore")]
pub mod localstore;
#[cfg(feature = "codegraph")]
pub mod codegraph;
#[cfg(feature = "fsindex")]
pub mod fsindex;
#[cfg(feature = "memory")]
pub mod memory;
#[cfg(feature = "ocr")]
pub mod ocr;
