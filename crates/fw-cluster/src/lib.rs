#![forbid(unsafe_code)]

//! `fw-cluster`: request router over the hosts of `cluster/build.toml` (`HybridClusterRouter::discover`) or the nodes of a Kubernetes cluster.
//! It implements, in this crate and without Pingora, dynamo or vllm-router as dependencies:
//! - consistent-hash choice of a proxy node, with health state;
//! - KV-cache prefix affinity and an llm-d style filter/score/pick scheduler (`epp`);
//! - prefill/decode disaggregation across GPU nodes;
//! - browser WebGPU peers registered through `POST /cluster/peers` (`webrtc://` endpoints are labels; there is no WebRTC or WebSocket transport).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub mod epp;
#[cfg(feature = "kube")]
pub mod k8s;

use epp::{Endpoint, Scheduler};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    BaseCudaPc,
    GpuHost,
    VpsProxyLbStorageCpu,
    UserWebGpuFpu,
    CloudflareEdge,
    GCloudAccelerator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: String,
    pub kind: NodeKind,
    pub endpoint: String,
    pub rpc_addr: Option<String>,
    pub gpu_model: String,
    pub vram_gb: u32,
    pub ram_gb: u32,
    pub cpu_cores: u32,
    pub healthy: bool,
    pub inflight: u32,
    /// Requests waiting in the engine queue.
    #[serde(default)]
    pub queue_depth: u32,
    /// Estimated prompt tokens in flight.
    #[serde(default)]
    pub inflight_tokens: u64,
    pub prefill_score: f32,
    pub decode_score: f32,
    pub cached_prefixes: Vec<String>,
}

/// Disaggregated prefill/decode and KV-cache routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub prefix_hash: String,
    pub kv_cache_hit: bool,
    pub proxy_vps: String,
    pub prefill_node: String,
    pub decode_node: String,
    pub draft_user_webgpu: Option<String>,
    pub disaggregated: bool,
    pub strategy: String,
    /// Leading prompt blocks already cached on the prefill node.
    pub prefix_matched_blocks: usize,
}

/// Consistent-hash load balancer and KV-cache prefix router.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridClusterRouter {
    pub cluster_name: String,
    pub nodes: Vec<ClusterNode>,
    /// `prefix_key` -> prefill node of the last request (compact view; the block index lives in `scheduler`).
    pub kv_prefix_index: HashMap<String, String>,
    /// llm-d EPP pipeline: Filter -> Score -> Pick, P/D handler, block-hash KV index.
    #[serde(skip)]
    pub scheduler: Scheduler,
}

impl HybridClusterRouter {
    /// Router over the hosts of a build cluster file (`cluster/build.toml`): one CPU node per `[hosts.*]` table, with
    /// the address, `vcpu` and the `[fw] serve_port` the file declares. Nothing else is assumed: RAM and GPUs are
    /// not declared there, so they stay zero until a node reports them. Scores scale with `vcpu`.
    pub fn from_build_cluster(cluster_name: &str, text: &str) -> Result<Self, String> {
        let doc: toml::Table = text.parse().map_err(|e| format!("build cluster file: {e}"))?;
        let serve_port = doc
            .get("fw")
            .and_then(|fw| fw.get("serve_port"))
            .and_then(toml::Value::as_integer)
            .ok_or("build cluster file: [fw] serve_port is missing")?;
        let hosts = doc
            .get("hosts")
            .and_then(toml::Value::as_table)
            .ok_or("build cluster file: [hosts] is missing")?;
        let mut declared = Vec::new();
        for (name, host) in hosts {
            let address = host
                .get("address")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| format!("build cluster file: hosts.{name}.address is missing"))?;
            let vcpu = host
                .get("vcpu")
                .and_then(toml::Value::as_integer)
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| format!("build cluster file: hosts.{name}.vcpu is missing"))?;
            declared.push((name.clone(), address.to_string(), vcpu));
        }
        let widest = declared.iter().map(|(_, _, vcpu)| *vcpu).max().unwrap_or(1).max(1);
        let nodes = declared
            .into_iter()
            .map(|(name, address, vcpu)| {
                let weight = vcpu as f32 / widest as f32;
                ClusterNode {
                    id: name,
                    kind: NodeKind::VpsProxyLbStorageCpu,
                    endpoint: format!("http://{address}:{serve_port}"),
                    rpc_addr: None,
                    gpu_model: "none".into(),
                    vram_gb: 0,
                    ram_gb: 0,
                    cpu_cores: vcpu,
                    healthy: true,
                    inflight: 0,
                    queue_depth: 0,
                    inflight_tokens: 0,
                    prefill_score: weight,
                    decode_score: weight,
                    cached_prefixes: Vec::new(),
                }
            })
            .collect();
        Ok(Self::from_nodes(cluster_name, nodes))
    }

    /// The router of the running fw: the hosts of `cluster/build.toml` (found through `$FW_ROOT` or the nearest
    /// ancestor of the working directory) plus the `local` node when the caller measured one. Without the file the
    /// router holds only the local node, so a status never reports hosts that were not read from a file.
    pub fn discover(local: Option<ClusterNode>) -> Self {
        let mut router = Self::build_cluster_text()
            .and_then(|text| Self::from_build_cluster("build-cluster", &text).ok())
            .unwrap_or_else(|| Self::from_nodes("local", Vec::new()));
        if let Some(node) = local {
            router.nodes.insert(0, node);
        }
        router
    }

    fn build_cluster_text() -> Option<String> {
        let mut roots: Vec<std::path::PathBuf> = std::env::var_os("FW_ROOT").map(std::path::PathBuf::from).into_iter().collect();
        let mut cwd = std::env::current_dir().ok();
        while let Some(dir) = cwd {
            cwd = dir.parent().map(std::path::Path::to_path_buf);
            roots.push(dir);
        }
        roots
            .iter()
            .find_map(|root| std::fs::read_to_string(root.join("cluster").join("build.toml")).ok())
    }

    /// The node this process runs on, from the cores and accelerator the caller measured.
    pub fn local_node(logical_cpus: usize, has_cuda: bool) -> ClusterNode {
        ClusterNode {
            id: "local".into(),
            kind: if has_cuda { NodeKind::BaseCudaPc } else { NodeKind::VpsProxyLbStorageCpu },
            endpoint: "http://127.0.0.1".into(),
            rpc_addr: None,
            gpu_model: if has_cuda { "CUDA device".into() } else { "none".into() },
            vram_gb: 0,
            ram_gb: 0,
            cpu_cores: u32::try_from(logical_cpus).unwrap_or(u32::MAX),
            healthy: true,
            inflight: 0,
            queue_depth: 0,
            inflight_tokens: 0,
            prefill_score: 1.0,
            decode_score: 1.0,
            cached_prefixes: Vec::new(),
        }
    }

    /// A router over the given nodes (for example the nodes of a Kubernetes cluster, see `k8s::router_nodes`).
    pub fn from_nodes(cluster_name: &str, nodes: Vec<ClusterNode>) -> Self {
        Self {
            cluster_name: cluster_name.into(),
            nodes,
            kv_prefix_index: HashMap::new(),
            scheduler: Scheduler::default(),
        }
    }

    pub fn register_user_webgpu_peer(&mut self, peer_id: &str, vram_gb: u32, cpu_cores: u32) {
        if self.nodes.iter().any(|n| n.id == peer_id) {
            return;
        }
        self.nodes.push(ClusterNode {
            id: peer_id.to_string(),
            kind: NodeKind::UserWebGpuFpu,
            endpoint: format!("webrtc://{peer_id}/fw-compute"),
            rpc_addr: None,
            gpu_model: "End-User Browser WebGPU + FPU".into(),
            vram_gb,
            ram_gb: 8,
            cpu_cores,
            healthy: true,
            inflight: 0,
            queue_depth: 0,
            inflight_tokens: 0,
            prefill_score: 0.5,
            decode_score: 0.7,
            cached_prefixes: Vec::new(),
        });
    }

    /// Hash prompt prefix for KV-cache radix affinity (vllm-router / dynamo style).
    pub fn prefix_key(prompt: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let prefix = &prompt[..prompt.len().min(128)];
        let digest = Sha256::digest(prefix.as_bytes());
        let mut out = String::with_capacity(16);
        for &b in &digest[..8] {
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
        out
    }

    /// Select the ingress proxy host, KV-affinity Prefill node, Decode node, and optional User WebGPU draft node.
    pub fn route_request(
        &mut self,
        prompt: &str,
        prefer_user_webgpu_draft: bool,
    ) -> RoutingDecision {
        let pkey = Self::prefix_key(prompt);

        // 1. Consistent-hash proxy selection across the CPU hosts
        let vps_nodes: Vec<&ClusterNode> = self
            .nodes
            .iter()
            .filter(|n| n.healthy && n.kind == NodeKind::VpsProxyLbStorageCpu)
            .collect();
        let hash_val = u64::from_str_radix(&pkey, 16).unwrap_or(0) as usize;
        let proxy_vps = if vps_nodes.is_empty() {
            "local-direct".to_string()
        } else {
            vps_nodes[hash_val % vps_nodes.len()].id.clone()
        };

        // 2. EPP pipeline: prefill profile (prefix-cache, token-load, queue-depth, capability), then decode profile
        let endpoints: Vec<Endpoint> = self.nodes.iter().map(Endpoint::from).collect();
        let (prefill_node, decode_node, kv_hit, matched) =
            match self.scheduler.schedule(prompt, &endpoints) {
                Some(d) => (
                    d.prefill.id,
                    d.decode.id,
                    d.prefill.prefix_matched_blocks > 0,
                    d.prefill.prefix_matched_blocks,
                ),
                None => (
                    "local-direct".to_string(),
                    "local-direct".to_string(),
                    false,
                    0,
                ),
            };
        if prefill_node != "local-direct" {
            self.scheduler.observe(&prefill_node, prompt);
            self.kv_prefix_index
                .insert(pkey.clone(), prefill_node.clone());
        }

        // 3. Optional End-User Browser WebGPU/FPU speculative draft peer
        let draft_user_webgpu = if prefer_user_webgpu_draft {
            self.nodes
                .iter()
                .find(|n| n.healthy && n.kind == NodeKind::UserWebGpuFpu)
                .map(|n| n.id.clone())
        } else {
            None
        };

        let disaggregated = prefill_node != decode_node;
        RoutingDecision {
            prefix_hash: pkey,
            kv_cache_hit: kv_hit,
            proxy_vps,
            prefill_node,
            decode_node,
            draft_user_webgpu,
            disaggregated,
            strategy: "consistent-hash + epp(filter,score,pick) + pd-profile".into(),
            prefix_matched_blocks: matched,
        }
    }

    /// Feed an engine KV-cache event (stored/removed/cleared blocks) into the prefix index.
    pub fn apply_kv_event(&mut self, event: epp::KvEvent) {
        self.scheduler.apply_kv_event(event);
    }

    /// Update the live load signals scorers read for one node.
    pub fn report_load(&mut self, node_id: &str, queue_depth: u32, inflight_tokens: u64) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == node_id) {
            n.queue_depth = queue_depth;
            n.inflight_tokens = inflight_tokens;
        }
    }

    pub fn summary(&self) -> serde_json::Value {
        let gpu_count = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::BaseCudaPc | NodeKind::GpuHost))
            .count();
        let vps_cpu_count = self
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::VpsProxyLbStorageCpu)
            .count();
        let user_webgpu_count = self
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::UserWebGpuFpu)
            .count();
        let total_vram_gb: u32 = self.nodes.iter().map(|n| n.vram_gb).sum();
        let total_ram_gb: u32 = self.nodes.iter().map(|n| n.ram_gb).sum();
        let total_cpu_cores: u32 = self.nodes.iter().map(|n| n.cpu_cores).sum();
        serde_json::json!({
            "cluster": self.cluster_name,
            "router": "fw-cluster: consistent-hash proxy choice + llm-d style filter/score/pick + prefill/decode profiles",
            "gpuHosts": gpu_count,
            "vpsProxyLbStorageCpuHosts": vps_cpu_count,
            "userWebGpuFpuPeers": user_webgpu_count,
            "totalVramGb": total_vram_gb,
            "totalRamGb": total_ram_gb,
            "totalCpuCores": total_cpu_cores,
            "nodes": self.nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD_CLUSTER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cluster/build.toml");

    fn build_cluster() -> HybridClusterRouter {
        let text = std::fs::read_to_string(BUILD_CLUSTER).expect("cluster/build.toml");
        HybridClusterRouter::from_build_cluster("build-cluster", &text).expect("parse cluster/build.toml")
    }

    #[test]
    fn nodes_are_the_hosts_the_build_cluster_file_declares() {
        let text = std::fs::read_to_string(BUILD_CLUSTER).expect("cluster/build.toml");
        let doc: toml::Table = text.parse().expect("toml");
        let hosts = doc["hosts"].as_table().expect("hosts");
        let router = build_cluster();
        assert_eq!(router.nodes.len(), hosts.len());
        for node in &router.nodes {
            let host = hosts[&node.id].as_table().expect("host table");
            let address = host["address"].as_str().expect("address");
            assert!(node.endpoint.starts_with(&format!("http://{address}:")), "{}", node.endpoint);
            assert_eq!(i64::from(node.cpu_cores), host["vcpu"].as_integer().expect("vcpu"));
        }
        let summary = router.summary();
        assert_eq!(summary["gpuHosts"], 0);
        assert_eq!(summary["vpsProxyLbStorageCpuHosts"], hosts.len());
        assert_eq!(summary["userWebGpuFpuPeers"], 0);
    }

    #[test]
    fn missing_declarations_are_errors_not_defaults() {
        assert!(HybridClusterRouter::from_build_cluster("x", "[hosts.a]\naddress = \"h\"\nvcpu = 2\n").is_err());
        assert!(HybridClusterRouter::from_build_cluster("x", "[fw]\nserve_port = 1\n").is_err());
        assert!(HybridClusterRouter::from_build_cluster("x", "[fw]\nserve_port = 1\n[hosts.a]\nvcpu = 2\n").is_err());
    }

    #[test]
    fn routes_kv_cache_and_disaggregated_prefill_decode_across_declared_hosts_and_a_browser_peer() {
        let mut router = build_cluster();
        router.register_user_webgpu_peer("peer-1", 4, 4);

        let d1 = router.route_request("System prompt: Fw ggml-rpc hybrid cluster", true);
        assert!(!d1.kv_cache_hit);
        assert!(d1.disaggregated);
        assert!(router.nodes.iter().any(|n| n.id == d1.proxy_vps));
        assert_eq!(d1.draft_user_webgpu.as_deref(), Some("peer-1"));

        // Second request with same prefix MUST hit KV-cache radix index
        let d2 = router.route_request("System prompt: Fw ggml-rpc hybrid cluster", true);
        assert!(d2.kv_cache_hit);
        assert_eq!(d2.prefill_node, d1.prefill_node);
    }

    #[test]
    fn a_local_node_is_a_gpu_host_only_when_cuda_was_measured() {
        let mut router = build_cluster();
        router.nodes.insert(0, HybridClusterRouter::local_node(16, true));
        assert_eq!(router.summary()["gpuHosts"], 1);
        assert_eq!(HybridClusterRouter::local_node(16, false).kind, NodeKind::VpsProxyLbStorageCpu);
    }
}
