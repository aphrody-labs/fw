//! llm-d Endpoint Picker (EPP) scheduling model: each profile is a chain of `Filter`s, a set of
//! weighted `Scorer`s and a `Picker`; the P/D handler runs a prefill profile, then a decode
//! profile over the remaining endpoints (`llm-d/llm-d-router`, `pkg/epp/framework/plugins/scheduling`).
//!
//! Scorer semantics follow upstream: prefix-cache = matched blocks / total blocks, token-load =
//! `1 - min(1, tokens / threshold)`, queue-depth = min-max normalised (equal queues score `1.0`),
//! picker = max score with round-robin rotation among ties.

use crate::{ClusterNode, NodeKind};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const TIE_EPSILON: f64 = 1e-9;

/// Scheduler view of a serving endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub id: String,
    pub kind: NodeKind,
    pub healthy: bool,
    pub queue_depth: u32,
    pub inflight_requests: u32,
    pub inflight_tokens: u64,
    pub prefill_score: f32,
    pub decode_score: f32,
}

impl From<&ClusterNode> for Endpoint {
    fn from(n: &ClusterNode) -> Self {
        Self {
            id: n.id.clone(),
            kind: n.kind,
            healthy: n.healthy,
            queue_depth: n.queue_depth,
            inflight_requests: n.inflight,
            inflight_tokens: n.inflight_tokens,
            prefill_score: n.prefill_score,
            decode_score: n.decode_score,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Prefill,
    Decode,
}

/// A request reduced to what scorers need: chained prefix block hashes and a token estimate.
#[derive(Debug, Clone)]
pub struct Request {
    pub block_hashes: Vec<u64>,
    pub prompt_tokens: u64,
}

/// Per-profile-run context handed to filters and scorers.
pub struct Ctx<'a> {
    pub request: &'a Request,
    pub index: &'a PrefixIndex,
    pub stage: Stage,
}

pub trait Filter: Debug + Send + Sync {
    fn keep(&self, ctx: &Ctx, endpoint: &Endpoint) -> bool;
}

/// Returns one score in `[0, 1]` per endpoint, aligned with the input slice.
pub trait Scorer: Debug + Send + Sync {
    fn score(&self, ctx: &Ctx, endpoints: &[Endpoint]) -> Vec<f64>;
}

pub trait Picker: Debug + Send + Sync {
    fn pick(&self, endpoints: &[Endpoint], scores: &[f64]) -> Option<usize>;
}

/// Drops unhealthy endpoints.
#[derive(Debug, Default)]
pub struct HealthyFilter;

impl Filter for HealthyFilter {
    fn keep(&self, _: &Ctx, e: &Endpoint) -> bool {
        e.healthy
    }
}

/// Keeps only the listed node kinds.
#[derive(Debug)]
pub struct KindFilter(pub Vec<NodeKind>);

impl Filter for KindFilter {
    fn keep(&self, _: &Ctx, e: &Endpoint) -> bool {
        self.0.contains(&e.kind)
    }
}

/// Saturation filter: drops endpoints whose waiting queue reached `max_queue` (llm-d `utilization-filter`).
#[derive(Debug)]
pub struct MaxQueueFilter(pub u32);

impl Filter for MaxQueueFilter {
    fn keep(&self, _: &Ctx, e: &Endpoint) -> bool {
        e.queue_depth < self.0
    }
}

/// Matched prefix blocks / total prefix blocks, from the [`PrefixIndex`].
#[derive(Debug, Default)]
pub struct PrefixCacheScorer;

impl Scorer for PrefixCacheScorer {
    fn score(&self, ctx: &Ctx, eps: &[Endpoint]) -> Vec<f64> {
        let total = ctx.request.block_hashes.len();
        eps.iter()
            .map(|e| match total {
                0 => 0.0,
                _ => ctx.index.matched_blocks(&e.id, &ctx.request.block_hashes) as f64 / total as f64,
            })
            .collect()
    }
}

/// `1 - min(1, inflight_tokens / threshold)`.
#[derive(Debug)]
pub struct TokenLoadScorer {
    pub threshold_tokens: u64,
}

impl Default for TokenLoadScorer {
    fn default() -> Self {
        Self { threshold_tokens: 4_194_304 }
    }
}

impl Scorer for TokenLoadScorer {
    fn score(&self, _: &Ctx, eps: &[Endpoint]) -> Vec<f64> {
        let t = self.threshold_tokens.max(1) as f64;
        eps.iter().map(|e| 1.0 - (e.inflight_tokens as f64 / t).min(1.0)).collect()
    }
}

/// Min-max normalised waiting-queue depth: shortest queue `1.0`, longest `0.0`, all equal `1.0`.
#[derive(Debug, Default)]
pub struct QueueDepthScorer;

impl Scorer for QueueDepthScorer {
    fn score(&self, _: &Ctx, eps: &[Endpoint]) -> Vec<f64> {
        let min = eps.iter().map(|e| e.queue_depth).min().unwrap_or(0);
        let max = eps.iter().map(|e| e.queue_depth).max().unwrap_or(0);
        eps.iter()
            .map(|e| match max == min {
                true => 1.0,
                false => f64::from(max - e.queue_depth) / f64::from(max - min),
            })
            .collect()
    }
}

/// Relative hardware capability for the current stage (`prefill_score` / `decode_score`), scaled by the best candidate.
#[derive(Debug, Default)]
pub struct CapabilityScorer;

impl Scorer for CapabilityScorer {
    fn score(&self, ctx: &Ctx, eps: &[Endpoint]) -> Vec<f64> {
        let cap = |e: &Endpoint| f64::from(match ctx.stage {
            Stage::Prefill => e.prefill_score,
            Stage::Decode => e.decode_score,
        });
        let best = eps.iter().map(cap).fold(0.0, f64::max);
        eps.iter().map(|e| if best > 0.0 { cap(e) / best } else { 0.0 }).collect()
    }
}

/// Highest score wins; equal scores rotate round-robin over the tied endpoints ordered by id.
#[derive(Debug, Default)]
pub struct MaxScorePicker {
    rotation: AtomicUsize,
}

impl Picker for MaxScorePicker {
    fn pick(&self, eps: &[Endpoint], scores: &[f64]) -> Option<usize> {
        let best = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut tied: Vec<usize> = (0..eps.len()).filter(|&i| scores[i] >= best - TIE_EPSILON).collect();
        tied.sort_by(|&a, &b| eps[a].id.cmp(&eps[b].id));
        (!tied.is_empty()).then(|| tied[self.rotation.fetch_add(1, Ordering::Relaxed) % tied.len()])
    }
}

/// Outcome of one profile run.
#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub id: String,
    /// Weighted score in `[0, 1]`.
    pub score: f64,
    pub prefix_matched_blocks: usize,
}

/// Filter -> Score -> Pick pipeline.
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    filters: Vec<Arc<dyn Filter>>,
    scorers: Vec<(Arc<dyn Scorer>, f64)>,
    picker: Arc<dyn Picker>,
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            filters: Vec::new(),
            scorers: Vec::new(),
            picker: Arc::new(MaxScorePicker::default()),
        }
    }

    pub fn filter(mut self, f: impl Filter + 'static) -> Self {
        self.filters.push(Arc::new(f));
        self
    }

    pub fn scorer(mut self, s: impl Scorer + 'static, weight: f64) -> Self {
        self.scorers.push((Arc::new(s), weight));
        self
    }

    pub fn picker(mut self, p: impl Picker + 'static) -> Self {
        self.picker = Arc::new(p);
        self
    }

    pub fn schedule(&self, ctx: &Ctx, endpoints: &[Endpoint]) -> Option<Pick> {
        let candidates: Vec<Endpoint> = endpoints
            .iter()
            .filter(|e| self.filters.iter().all(|f| f.keep(ctx, e)))
            .cloned()
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let weight_sum: f64 = self.scorers.iter().map(|(_, w)| w).sum();
        let mut totals = vec![0.0; candidates.len()];
        for (scorer, weight) in &self.scorers {
            for (t, s) in totals.iter_mut().zip(scorer.score(ctx, &candidates)) {
                *t += weight * s.clamp(0.0, 1.0);
            }
        }
        if weight_sum > 0.0 {
            totals.iter_mut().for_each(|t| *t /= weight_sum);
        }
        let i = self.picker.pick(&candidates, &totals)?;
        Some(Pick {
            prefix_matched_blocks: ctx.index.matched_blocks(&candidates[i].id, &ctx.request.block_hashes),
            id: candidates[i].id.clone(),
            score: totals[i],
        })
    }
}

/// Prefill profile: prefix-cache affinity dominates, then load, then hardware capability.
pub fn default_prefill_profile() -> Profile {
    Profile::new("prefill")
        .filter(HealthyFilter)
        .filter(KindFilter(inference_kinds()))
        .scorer(PrefixCacheScorer, 3.0)
        .scorer(TokenLoadScorer::default(), 1.0)
        .scorer(QueueDepthScorer, 1.0)
        .scorer(CapabilityScorer, 1.0)
}

/// Decode profile: KV arrives by transfer, so prefix affinity is not scored.
pub fn default_decode_profile() -> Profile {
    Profile::new("decode")
        .filter(HealthyFilter)
        .filter(KindFilter(inference_kinds()))
        .scorer(TokenLoadScorer::default(), 2.0)
        .scorer(QueueDepthScorer, 1.0)
        .scorer(CapabilityScorer, 1.0)
}

fn inference_kinds() -> Vec<NodeKind> {
    vec![NodeKind::BaseCudaPc, NodeKind::GpuHost, NodeKind::VpsProxyLbStorageCpu]
}

/// KV-cache event from an engine, expressed as chained block hashes of [`PrefixIndex::block_hashes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KvEvent {
    Stored { endpoint: String, blocks: Vec<u64> },
    Removed { endpoint: String, blocks: Vec<u64> },
    Cleared { endpoint: String },
}

#[derive(Debug, Clone, Default)]
struct EndpointBlocks {
    set: HashSet<u64>,
    order: VecDeque<u64>,
}

/// Approximate global KV-cache index: which endpoint holds which chained prefix blocks.
/// Fed by [`PrefixIndex::observe`] (routing decisions) or [`PrefixIndex::apply`] (engine events);
/// each endpoint keeps at most `capacity` blocks, oldest evicted first.
#[derive(Debug, Clone)]
pub struct PrefixIndex {
    block_size: usize,
    capacity: usize,
    endpoints: HashMap<String, EndpointBlocks>,
}

impl Default for PrefixIndex {
    fn default() -> Self {
        Self::new(64, 65_536)
    }
}

impl PrefixIndex {
    /// `block_size` in prompt bytes (about 16 tokens at 4 bytes per token).
    pub fn new(block_size: usize, capacity: usize) -> Self {
        Self { block_size: block_size.max(1), capacity: capacity.max(1), endpoints: HashMap::new() }
    }

    /// Chained hashes: block `i` hashes block `i - 1`'s hash and its own bytes, so a match on
    /// block `i` implies the whole prefix up to `i` matches. The tail block may be partial.
    pub fn block_hashes(&self, prompt: &str) -> Vec<u64> {
        let mut prev = 0u64;
        prompt
            .as_bytes()
            .chunks(self.block_size)
            .map(|chunk| {
                let mut h = Sha256::new();
                h.update(prev.to_le_bytes());
                h.update(chunk);
                prev = u64::from_le_bytes(h.finalize()[..8].try_into().expect("sha256 is 32 bytes"));
                prev
            })
            .collect()
    }

    pub fn request(&self, prompt: &str) -> Request {
        Request { block_hashes: self.block_hashes(prompt), prompt_tokens: (prompt.len() as u64).div_ceil(4) }
    }

    /// Record that `endpoint` now holds the KV blocks of `prompt`.
    pub fn observe(&mut self, endpoint: &str, prompt: &str) {
        let blocks = self.block_hashes(prompt);
        self.apply(KvEvent::Stored { endpoint: endpoint.into(), blocks });
    }

    pub fn apply(&mut self, event: KvEvent) {
        match event {
            KvEvent::Stored { endpoint, blocks } => {
                let cap = self.capacity;
                let eb = self.endpoints.entry(endpoint).or_default();
                for b in blocks {
                    if eb.set.insert(b) {
                        eb.order.push_back(b);
                    }
                }
                while eb.order.len() > cap {
                    if let Some(old) = eb.order.pop_front() {
                        eb.set.remove(&old);
                    }
                }
            }
            KvEvent::Removed { endpoint, blocks } => {
                if let Some(eb) = self.endpoints.get_mut(&endpoint) {
                    let gone: HashSet<u64> = blocks.into_iter().collect();
                    eb.set.retain(|b| !gone.contains(b));
                    eb.order.retain(|b| !gone.contains(b));
                }
            }
            KvEvent::Cleared { endpoint } => {
                self.endpoints.remove(&endpoint);
            }
        }
    }

    /// Number of leading request blocks cached on `endpoint`.
    pub fn matched_blocks(&self, endpoint: &str, hashes: &[u64]) -> usize {
        self.endpoints
            .get(endpoint)
            .map_or(0, |eb| hashes.iter().take_while(|h| eb.set.contains(h)).count())
    }
}

/// Result of the P/D profile handler.
#[derive(Debug, Clone, PartialEq)]
pub struct PdDecision {
    pub prefill: Pick,
    pub decode: Pick,
}

impl PdDecision {
    /// `true` when prefill and decode run on different endpoints.
    pub fn disaggregated(&self) -> bool {
        self.prefill.id != self.decode.id
    }
}

/// P/D profile handler: prefill profile first, then the decode profile over the other endpoints
/// (falls back to the prefill endpoint when nothing else qualifies).
#[derive(Debug, Clone)]
pub struct Scheduler {
    pub index: PrefixIndex,
    pub prefill: Profile,
    pub decode: Profile,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self { index: PrefixIndex::default(), prefill: default_prefill_profile(), decode: default_decode_profile() }
    }
}

impl Scheduler {
    pub fn schedule(&self, prompt: &str, endpoints: &[Endpoint]) -> Option<PdDecision> {
        let request = self.index.request(prompt);
        let ctx = |stage| Ctx { request: &request, index: &self.index, stage };
        let prefill = self.prefill.schedule(&ctx(Stage::Prefill), endpoints)?;
        let others: Vec<Endpoint> = endpoints.iter().filter(|e| e.id != prefill.id).cloned().collect();
        let decode = self
            .decode
            .schedule(&ctx(Stage::Decode), &others)
            .unwrap_or_else(|| Pick { id: prefill.id.clone(), ..prefill.clone() });
        Some(PdDecision { prefill, decode })
    }

    pub fn observe(&mut self, endpoint: &str, prompt: &str) {
        self.index.observe(endpoint, prompt);
    }

    pub fn apply_kv_event(&mut self, event: KvEvent) {
        self.index.apply(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(id: &str) -> Endpoint {
        Endpoint {
            id: id.into(),
            kind: NodeKind::GpuHost,
            healthy: true,
            queue_depth: 0,
            inflight_requests: 0,
            inflight_tokens: 0,
            prefill_score: 1.0,
            decode_score: 1.0,
        }
    }

    const LONG: &str = "You are a precise assistant. Answer using the repository context below and cite file paths. \
                        Context: crates/fw-cluster routes requests across a hybrid cluster of GPU hosts and CPU nodes.";

    #[test]
    fn prefix_affinity_sticks_to_the_same_endpoint() {
        let mut s = Scheduler::default();
        let eps = [ep("a"), ep("b"), ep("c")];
        let first = s.schedule(LONG, &eps).unwrap();
        assert_eq!(first.prefill.prefix_matched_blocks, 0);
        s.observe(&first.prefill.id, LONG);
        for _ in 0..5 {
            let again = s.schedule(&format!("{LONG} Question {}", 7), &eps).unwrap();
            assert_eq!(again.prefill.id, first.prefill.id);
            assert!(again.prefill.prefix_matched_blocks >= 2);
        }
        // an unrelated prompt is free to go elsewhere
        let other = s.schedule("completely different short prompt", &eps).unwrap();
        assert_eq!(other.prefill.prefix_matched_blocks, 0);
    }

    #[test]
    fn affinity_survives_moderate_load_but_not_an_unhealthy_endpoint() {
        let mut s = Scheduler::default();
        let mut eps = [ep("a"), ep("b"), ep("c")];
        s.observe("b", LONG);
        eps[1].queue_depth = 3;
        eps[1].inflight_tokens = 20_000;
        assert_eq!(s.schedule(LONG, &eps).unwrap().prefill.id, "b");
        eps[1].healthy = false;
        assert_ne!(s.schedule(LONG, &eps).unwrap().prefill.id, "b");
    }

    #[test]
    fn kv_events_update_the_index() {
        let mut s = Scheduler::default();
        let blocks = s.index.block_hashes(LONG);
        s.apply_kv_event(KvEvent::Stored { endpoint: "c".into(), blocks: blocks.clone() });
        assert_eq!(s.index.matched_blocks("c", &blocks), blocks.len());
        s.apply_kv_event(KvEvent::Removed { endpoint: "c".into(), blocks: vec![blocks[0]] });
        assert_eq!(s.index.matched_blocks("c", &blocks), 0, "prefix chain is broken at block 0");
        s.apply_kv_event(KvEvent::Stored { endpoint: "c".into(), blocks });
        s.apply_kv_event(KvEvent::Cleared { endpoint: "c".into() });
        assert_eq!(s.index.matched_blocks("c", &s.index.block_hashes(LONG)), 0);
    }

    #[test]
    fn index_capacity_evicts_oldest_blocks() {
        let mut idx = PrefixIndex::new(4, 3);
        idx.observe("a", "aaaabbbbccccdddd");
        let h = idx.block_hashes("aaaabbbbccccdddd");
        assert_eq!(idx.matched_blocks("a", &h), 0, "block 0 evicted");
        assert!(idx.endpoints["a"].set.contains(&h[3]));
        assert_eq!(idx.endpoints["a"].set.len(), 3);
    }

    #[test]
    fn load_spreads_across_equal_endpoints() {
        let s = Scheduler::default();
        let mut eps = [ep("a"), ep("b"), ep("c")];
        let mut counts = HashMap::new();
        for i in 0..30 {
            let prompt = format!("distinct prompt number {i} with no shared prefix");
            let d = s.schedule(&prompt, &eps).unwrap();
            *counts.entry(d.prefill.id.clone()).or_insert(0) += 1;
            let e = eps.iter_mut().find(|e| e.id == d.prefill.id).unwrap();
            e.inflight_tokens += 4_000;
            e.queue_depth += 1;
        }
        assert!(counts.values().all(|&c| (9..=11).contains(&c)), "{counts:?}");
    }

    #[test]
    fn filters_exclude_unhealthy_wrong_kind_and_saturated() {
        let index = PrefixIndex::default();
        let req = index.request("hi");
        let ctx = Ctx { request: &req, index: &index, stage: Stage::Prefill };
        let mut sick = ep("sick");
        sick.healthy = false;
        let mut edge = ep("edge");
        edge.kind = NodeKind::CloudflareEdge;
        let mut busy = ep("busy");
        busy.queue_depth = 5;
        let eps = [sick, edge, busy, ep("ok")];

        let p = Profile::new("t").filter(HealthyFilter).filter(KindFilter(vec![NodeKind::GpuHost])).filter(MaxQueueFilter(5)).scorer(QueueDepthScorer, 1.0);
        assert_eq!(p.schedule(&ctx, &eps).unwrap().id, "ok");
        assert!(p.schedule(&ctx, &eps[..3]).is_none());
    }

    #[test]
    fn pd_returns_two_distinct_endpoints_and_falls_back_to_one() {
        let s = Scheduler::default();
        let d = s.schedule(LONG, &[ep("a"), ep("b"), ep("c")]).unwrap();
        assert!(d.disaggregated());
        assert_ne!(d.prefill.id, d.decode.id);

        let solo = s.schedule(LONG, &[ep("a")]).unwrap();
        assert!(!solo.disaggregated());
        assert_eq!(solo.decode.id, "a");
        assert!(s.schedule(LONG, &[]).is_none());
    }

    #[test]
    fn pd_picks_capable_prefill_and_least_loaded_decode() {
        let s = Scheduler::default();
        let mut strong = ep("strong");
        strong.prefill_score = 2.0;
        let mut loaded = ep("loaded");
        loaded.inflight_tokens = 1_000_000;
        loaded.queue_depth = 4;
        let d = s.schedule(LONG, &[strong, loaded, ep("idle")]).unwrap();
        assert_eq!(d.prefill.id, "strong");
        assert_eq!(d.decode.id, "idle");
    }

    #[test]
    fn scorers_follow_upstream_formulas() {
        let index = PrefixIndex::default();
        let req = index.request("x");
        let ctx = Ctx { request: &req, index: &index, stage: Stage::Decode };
        let mut a = ep("a");
        a.queue_depth = 2;
        let mut b = ep("b");
        b.queue_depth = 6;
        b.inflight_tokens = 2_097_152;
        assert_eq!(QueueDepthScorer.score(&ctx, &[a.clone(), b.clone()]), [1.0, 0.0]);
        assert_eq!(QueueDepthScorer.score(&ctx, &[a.clone(), a.clone()]), [1.0, 1.0]);
        assert_eq!(TokenLoadScorer::default().score(&ctx, &[a, b]), [1.0, 0.5]);
    }
}
