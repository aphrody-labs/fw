//! Kubernetes through kube-rs (`kube` 5.1, `k8s-openapi` 0.29): node inventory, pods, server-side apply of
//! YAML manifests, and the cluster's nodes turned into router nodes so `HybridClusterRouter` schedules onto
//! a real cluster. The client reads the usual kubeconfig (`KUBECONFIG`, `~/.kube/config`) or in-cluster env.

use crate::{ClusterNode, HybridClusterRouter, NodeKind};
use k8s_openapi::api::core::v1::{Node, Pod};
use kube::api::{Api, DynamicObject, ListParams, Patch, PatchParams};
use kube::core::GroupVersionKind;
use kube::discovery::{Discovery, Scope};
use kube::{Client, ResourceExt};
use serde::Serialize;
use std::collections::BTreeMap;

/// Field manager recorded on objects fw applies.
pub const FIELD_MANAGER: &str = "fw";

/// Extended resources that count as GPUs.
const GPU_RESOURCES: [&str; 3] = ["nvidia.com/gpu", "amd.com/gpu", "gpu.intel.com/i915"];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeInfo {
    pub name: String,
    pub ready: bool,
    pub schedulable: bool,
    pub roles: Vec<String>,
    pub kubelet: String,
    pub os_image: String,
    pub arch: String,
    pub internal_ip: Option<String>,
    pub cpu_cores: f64,
    pub memory_bytes: u64,
    pub gpus: u32,
    pub gpu_model: Option<String>,
    /// GPU memory per device in MiB, from GPU feature discovery labels when present.
    pub gpu_memory_mib: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PodInfo {
    pub namespace: String,
    pub name: String,
    pub phase: String,
    pub node: Option<String>,
    pub ready: String,
    pub restarts: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Applied {
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub resource_version: Option<String>,
}

#[derive(Debug)]
pub enum K8sError {
    Kube(kube::Error),
    Manifest(String),
}

impl std::fmt::Display for K8sError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kube(e) => write!(f, "kubernetes: {e}"),
            Self::Manifest(m) => write!(f, "manifest: {m}"),
        }
    }
}

impl std::error::Error for K8sError {}

impl From<kube::Error> for K8sError {
    fn from(e: kube::Error) -> Self {
        Self::Kube(e)
    }
}

pub type Result<T> = std::result::Result<T, K8sError>;

/// Parses a CPU quantity (`4`, `3500m`, `0.5`) into cores.
pub fn parse_cpu(q: &str) -> Option<f64> {
    let q = q.trim();
    match q.strip_suffix('m') {
        Some(milli) => milli.parse::<f64>().ok().map(|m| m / 1000.0),
        None => q.parse().ok(),
    }
}

/// Parses a memory quantity (`16Gi`, `16384Ki`, `2G`, `1e9`, `512`) into bytes.
pub fn parse_bytes(q: &str) -> Option<u64> {
    let q = q.trim();
    const SUFFIXES: [(&str, f64); 12] = [
        ("Ki", 1024.0),
        ("Mi", 1_048_576.0),
        ("Gi", 1_073_741_824.0),
        ("Ti", 1_099_511_627_776.0),
        ("Pi", 1_125_899_906_842_624.0),
        ("Ei", 1_152_921_504_606_846_976.0),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
        ("T", 1e12),
        ("P", 1e15),
        ("E", 1e18),
    ];
    for (suffix, mult) in SUFFIXES {
        if let Some(n) = q.strip_suffix(suffix) {
            return n.parse::<f64>().ok().map(|v| (v * mult) as u64);
        }
    }
    q.parse::<f64>().ok().map(|v| v as u64)
}

/// Facts about one node, from its status, capacity and labels.
pub fn node_info(node: &Node) -> NodeInfo {
    let labels = node.labels();
    let status = node.status.clone().unwrap_or_default();
    let capacity = status.capacity.unwrap_or_default();
    let get = |k: &str| capacity.get(k).map(|q| q.0.clone());
    let ready = status
        .conditions
        .unwrap_or_default()
        .iter()
        .any(|c| c.type_ == "Ready" && c.status == "True");
    let info = status.node_info.unwrap_or_default();
    let mut roles: Vec<String> = labels
        .keys()
        .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
        .map(str::to_owned)
        .collect();
    roles.sort();
    NodeInfo {
        name: node.name_any(),
        ready,
        schedulable: !node
            .spec
            .as_ref()
            .and_then(|s| s.unschedulable)
            .unwrap_or(false),
        roles,
        kubelet: info.kubelet_version,
        os_image: info.os_image,
        arch: info.architecture,
        internal_ip: status
            .addresses
            .unwrap_or_default()
            .into_iter()
            .find(|a| a.type_ == "InternalIP")
            .map(|a| a.address),
        cpu_cores: get("cpu").and_then(|c| parse_cpu(&c)).unwrap_or(0.0),
        memory_bytes: get("memory").and_then(|m| parse_bytes(&m)).unwrap_or(0),
        gpus: GPU_RESOURCES
            .iter()
            .filter_map(|r| get(r))
            .filter_map(|v| v.parse::<u32>().ok())
            .sum(),
        gpu_model: labels
            .get("nvidia.com/gpu.product")
            .or_else(|| labels.get("amd.com/gpu.product-name"))
            .cloned(),
        gpu_memory_mib: labels
            .get("nvidia.com/gpu.memory")
            .and_then(|m| m.parse().ok()),
    }
}

/// Router nodes for the cluster: GPU nodes serve prefill and decode, the others proxy and store.
pub fn router_nodes(nodes: &[NodeInfo]) -> Vec<ClusterNode> {
    nodes
        .iter()
        .map(|n| {
            let gpu = n.gpus > 0;
            ClusterNode {
                id: format!("k8s/{}", n.name),
                kind: if gpu {
                    NodeKind::GpuHost
                } else {
                    NodeKind::VpsProxyLbStorageCpu
                },
                endpoint: n
                    .internal_ip
                    .as_ref()
                    .map_or_else(|| format!("k8s://{}", n.name), |ip| format!("http://{ip}")),
                rpc_addr: None,
                gpu_model: n
                    .gpu_model
                    .clone()
                    .unwrap_or_else(|| if gpu { "GPU".into() } else { "none".into() }),
                vram_gb: n
                    .gpu_memory_mib
                    .map_or(0, |m| (m / 1024) as u32 * n.gpus.max(1)),
                ram_gb: (n.memory_bytes / 1_073_741_824) as u32,
                cpu_cores: n.cpu_cores.floor() as u32,
                healthy: n.ready && n.schedulable,
                inflight: 0,
                queue_depth: 0,
                inflight_tokens: 0,
                prefill_score: if gpu { 1.0 } else { 0.2 },
                decode_score: if gpu { 1.0 } else { 0.3 },
                cached_prefixes: Vec::new(),
            }
        })
        .collect()
}

pub async fn nodes(client: &Client) -> Result<Vec<NodeInfo>> {
    let list = Api::<Node>::all(client.clone())
        .list(&ListParams::default())
        .await?;
    Ok(list.items.iter().map(node_info).collect())
}

pub async fn pods(client: &Client, namespace: Option<&str>) -> Result<Vec<PodInfo>> {
    let api: Api<Pod> = match namespace {
        Some(ns) => Api::namespaced(client.clone(), ns),
        None => Api::all(client.clone()),
    };
    let list = api.list(&ListParams::default()).await?;
    Ok(list
        .items
        .iter()
        .map(|p| {
            let statuses = p
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.clone())
                .unwrap_or_default();
            PodInfo {
                namespace: p.namespace().unwrap_or_default(),
                name: p.name_any(),
                phase: p
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.clone())
                    .unwrap_or_else(|| "Unknown".into()),
                node: p.spec.as_ref().and_then(|s| s.node_name.clone()),
                ready: format!(
                    "{}/{}",
                    statuses.iter().filter(|c| c.ready).count(),
                    statuses.len()
                ),
                restarts: statuses.iter().map(|c| c.restart_count).sum(),
            }
        })
        .collect())
}

/// Server version, node readiness and pod phases in one JSON value.
pub async fn status(client: &Client) -> Result<serde_json::Value> {
    let version = client.apiserver_version().await?;
    let nodes = nodes(client).await?;
    let pods = pods(client, None).await?;
    let mut phases: BTreeMap<String, usize> = BTreeMap::new();
    for p in &pods {
        *phases.entry(p.phase.clone()).or_default() += 1;
    }
    Ok(serde_json::json!({
        "server": version.git_version,
        "platform": version.platform,
        "nodes": nodes.len(),
        "nodesReady": nodes.iter().filter(|n| n.ready).count(),
        "gpus": nodes.iter().map(|n| n.gpus).sum::<u32>(),
        "pods": pods.len(),
        "podPhases": phases,
    }))
}

/// Splits a multi-document YAML manifest into objects, skipping empty documents.
pub fn parse_manifest(yaml: &str) -> Result<Vec<DynamicObject>> {
    let docs: Vec<serde_json::Value> =
        serde_saphyr::from_multiple(yaml).map_err(|e| K8sError::Manifest(e.to_string()))?;
    docs.into_iter()
        .filter(|d| !d.is_null())
        .map(|d| {
            let obj: DynamicObject =
                serde_json::from_value(d).map_err(|e| K8sError::Manifest(e.to_string()))?;
            if obj.types.is_none() || obj.metadata.name.is_none() {
                return Err(K8sError::Manifest(
                    "every document needs apiVersion, kind and metadata.name".into(),
                ));
            }
            Ok(obj)
        })
        .collect()
}

/// Server-side apply of every object in `yaml` (field manager `fw`, conflicts forced), in order.
/// Namespaced objects without a namespace go to `default_namespace`.
pub async fn apply(client: &Client, yaml: &str, default_namespace: &str) -> Result<Vec<Applied>> {
    let objects = parse_manifest(yaml)?;
    let discovery = Discovery::new(client.clone()).run().await?;
    let params = PatchParams::apply(FIELD_MANAGER).force();
    let mut out = Vec::with_capacity(objects.len());
    for obj in objects {
        let types = obj.types.clone().unwrap_or_default();
        let gvk =
            GroupVersionKind::try_from(&types).map_err(|e| K8sError::Manifest(e.to_string()))?;
        let (resource, caps) = discovery.resolve_gvk(&gvk).ok_or_else(|| {
            K8sError::Manifest(format!(
                "the cluster does not serve {}/{}",
                types.api_version, types.kind
            ))
        })?;
        let name = obj.name_any();
        let namespace = (caps.scope == Scope::Namespaced).then(|| {
            obj.namespace()
                .unwrap_or_else(|| default_namespace.to_owned())
        });
        let api: Api<DynamicObject> = match &namespace {
            Some(ns) => Api::namespaced_with(client.clone(), ns, &resource),
            None => Api::all_with(client.clone(), &resource),
        };
        let result = api.patch(&name, &params, &Patch::Apply(&obj)).await?;
        out.push(Applied {
            kind: types.kind,
            namespace,
            name,
            resource_version: result.resource_version(),
        });
    }
    Ok(out)
}

/// A router over the cluster's nodes.
pub async fn router(client: &Client, cluster_name: &str) -> Result<HybridClusterRouter> {
    Ok(HybridClusterRouter::from_nodes(
        cluster_name,
        router_nodes(&nodes(client).await?),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities() {
        assert_eq!(parse_cpu("4"), Some(4.0));
        assert_eq!(parse_cpu("3500m"), Some(3.5));
        assert_eq!(parse_bytes("16Gi"), Some(17_179_869_184));
        assert_eq!(parse_bytes("16384Ki"), Some(16_777_216));
        assert_eq!(parse_bytes("2G"), Some(2_000_000_000));
        assert_eq!(parse_bytes("512"), Some(512));
        assert_eq!(parse_bytes("lots"), None);
    }

    fn node(json: serde_json::Value) -> Node {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn node_facts_and_router_nodes() {
        let gpu = node(serde_json::json!({
            "metadata": { "name": "gpu-0", "labels": {
                "node-role.kubernetes.io/worker": "", "nvidia.com/gpu.product": "NVIDIA-L4", "nvidia.com/gpu.memory": "23034" } },
            "status": {
                "capacity": { "cpu": "16", "memory": "65536Mi", "nvidia.com/gpu": "2" },
                "conditions": [{ "type": "Ready", "status": "True" }],
                "addresses": [{ "type": "InternalIP", "address": "10.0.0.5" }],
                "nodeInfo": { "kubeletVersion": "v1.34.2", "osImage": "Debian", "architecture": "amd64",
                  "bootID": "", "containerRuntimeVersion": "", "kernelVersion": "", "kubeProxyVersion": "", "machineID": "", "operatingSystem": "linux", "systemUUID": "" }
            }
        }));
        let cpu = node(serde_json::json!({
            "metadata": { "name": "cp", "labels": { "node-role.kubernetes.io/control-plane": "" } },
            "spec": { "unschedulable": true },
            "status": { "capacity": { "cpu": "4000m", "memory": "8Gi" }, "conditions": [{ "type": "Ready", "status": "False" }] }
        }));
        let infos = [node_info(&gpu), node_info(&cpu)];
        assert_eq!(infos[0].gpus, 2);
        assert_eq!(infos[0].gpu_model.as_deref(), Some("NVIDIA-L4"));
        assert_eq!(infos[0].roles, ["worker"]);
        assert_eq!(infos[0].memory_bytes, 68_719_476_736);
        assert!(infos[0].ready && infos[0].schedulable);
        assert!(!infos[1].ready && !infos[1].schedulable);

        let r = router_nodes(&infos);
        assert_eq!(r[0].id, "k8s/gpu-0");
        assert_eq!(r[0].kind, NodeKind::GpuHost);
        assert_eq!(r[0].endpoint, "http://10.0.0.5");
        assert_eq!((r[0].vram_gb, r[0].ram_gb, r[0].cpu_cores), (44, 64, 16));
        assert_eq!(r[1].kind, NodeKind::VpsProxyLbStorageCpu);
        assert!(!r[1].healthy);
        assert_eq!(r[1].cpu_cores, 4);
    }

    #[test]
    fn manifests_split_and_validate() {
        let objs = parse_manifest(
            "apiVersion: v1\nkind: Namespace\nmetadata:\n  name: fw\n---\n---\napiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: drive\n  namespace: fw\nspec:\n  replicas: 1\n",
        )
        .unwrap();
        assert_eq!(objs.len(), 2);
        assert_eq!(objs[1].types.as_ref().unwrap().kind, "Deployment");
        assert_eq!(objs[1].namespace().as_deref(), Some("fw"));
        assert!(matches!(
            parse_manifest("kind: Pod\n"),
            Err(K8sError::Manifest(_))
        ));
    }
}
