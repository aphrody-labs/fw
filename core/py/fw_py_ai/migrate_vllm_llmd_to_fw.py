"""Migrate vLLM or llm-d deployments to Fw `fw-cluster.json` with parity gap analysis.

CLI usage:
    python migrate_vllm_llmd_to_fw.py <vllm|llmd> [--source <path>] [--target <dir>] [--dry-run|--apply]

Supported `vllm` sources:
- vLLM CLI flags file (`.txt`, `.sh`, `.args`, or raw `vllm serve ...` command text)
- `vllm_config.json` (JSON object with engine/serving/kv_transfer_config fields)
- `docker-compose.yml` / `compose.yaml` (single or multi-service P/D stacks)

Supported `llmd` sources:
- `llm-d` Helm `values.yaml` (including embedded `EndpointPickerConfig` literal blocks)
- Kubernetes Gateway API Inference Extension `InferencePool` YAML/JSON
- Multi-document YAML combining `InferencePool`, `EndpointPickerConfig`, and `Deployment`

Pure Python 3.12+ standard library only (zero pip installs required).
"""

from __future__ import annotations

import argparse
import json
import re
import shlex
import sys
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

# ----------------------------------------------------------------------
# Minimal stdlib YAML subset parser (mappings, sequences, `|` blocks, `---`)
# ----------------------------------------------------------------------


def _strip_yaml_comment(line: str) -> str:
    in_single = False
    in_double = False
    for idx, ch in enumerate(line):
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif (
            ch == "#"
            and not in_single
            and not in_double
            and (idx == 0 or line[idx - 1].isspace())
        ):
            return line[:idx].rstrip()
    return line.rstrip()


def _parse_yaml_scalar(raw: str) -> Any:
    text = raw.strip()
    if not text or text in ("null", "Null", "NULL", "~"):
        return None
    if text in ("true", "True", "TRUE"):
        return True
    if text in ("false", "False", "FALSE"):
        return False
    if (text.startswith('"') and text.endswith('"')) or (
        text.startswith("'") and text.endswith("'")
    ):
        if text.startswith('"'):
            try:
                return json.loads(text)
            except json.JSONDecodeError:
                return text[1:-1]
        return text[1:-1]
    if (text.startswith("{") and text.endswith("}")) or (
        text.startswith("[") and text.endswith("]")
    ):
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            # Handle simple unquoted YAML flow lists like [a, b, c]
            if text.startswith("[") and text.endswith("]"):
                inner = text[1:-1].strip()
                if not inner:
                    return []
                return [_parse_yaml_scalar(part) for part in inner.split(",")]
            return text
    if re.fullmatch(r"-?\d+", text):
        return int(text)
    if re.fullmatch(r"-?\d+\.\d+", text):
        return float(text)
    return text


def _split_yaml_key_val(text: str) -> tuple[str, str] | None:
    in_single = False
    in_double = False
    for idx, ch in enumerate(text):
        if ch == "'" and not in_double:
            in_single = not in_single
        elif ch == '"' and not in_single:
            in_double = not in_double
        elif (
            ch == ":"
            and not in_single
            and not in_double
            and (idx + 1 == len(text) or text[idx + 1].isspace())
        ):
            key = text[:idx].strip().strip('"').strip("'")
            val = text[idx + 1 :].strip()
            return key, val
    return None


def _parse_yaml_block(
    lines: list[tuple[int, str]],
    start: int,
    min_indent: int,
) -> tuple[Any, int]:
    if start >= len(lines):
        return None, start
    first_indent, first_text = lines[start]
    if first_indent < min_indent:
        return None, start

    # Sequence block
    if first_text.startswith("- ") or first_text == "-":
        items: list[Any] = []
        idx = start
        while idx < len(lines):
            indent, text = lines[idx]
            if indent < first_indent:
                break
            if indent > first_indent or not (text.startswith("- ") or text == "-"):
                break
            rest = text[2:].strip() if text.startswith("- ") else ""
            idx += 1
            if not rest:
                child, idx = _parse_yaml_block(lines, idx, first_indent + 1)
                items.append(child)
                continue
            kv = _split_yaml_key_val(rest)
            if kv is not None:
                k, v = kv
                entry: dict[str, Any] = {}
                if v in ("|", "|-", ">", ">-"):
                    block_val, idx = _consume_literal_block(lines, idx, first_indent + 2)
                    entry[k] = block_val
                elif v:
                    entry[k] = _parse_yaml_scalar(v)
                else:
                    child, idx = _parse_yaml_block(lines, idx, first_indent + 2)
                    entry[k] = child
                # Collect sibling keys indented under this sequence item
                while idx < len(lines) and lines[idx][0] > first_indent:
                    sub_indent, sub_text = lines[idx]
                    sub_kv = _split_yaml_key_val(sub_text)
                    if sub_kv is None:
                        break
                    sk, sv = sub_kv
                    idx += 1
                    if sv in ("|", "|-", ">", ">-"):
                        entry[sk], idx = _consume_literal_block(lines, idx, sub_indent + 1)
                    elif sv:
                        entry[sk] = _parse_yaml_scalar(sv)
                    else:
                        entry[sk], idx = _parse_yaml_block(lines, idx, sub_indent + 1)
                items.append(entry)
            else:
                items.append(_parse_yaml_scalar(rest))
        return items, idx

    # Mapping block
    mapping: dict[str, Any] = {}
    idx = start
    while idx < len(lines):
        indent, text = lines[idx]
        if indent < first_indent:
            break
        if indent > first_indent:
            idx += 1
            continue
        kv = _split_yaml_key_val(text)
        if kv is None:
            break
        key, val = kv
        idx += 1
        if val in ("|", "|-", ">", ">-"):
            mapping[key], idx = _consume_literal_block(lines, idx, first_indent + 1)
        elif val:
            mapping[key] = _parse_yaml_scalar(val)
        else:
            if idx < len(lines) and lines[idx][0] >= first_indent:
                next_indent, next_text = lines[idx]
                if next_indent > first_indent or (
                    next_indent == first_indent
                    and (next_text.startswith("- ") or next_text == "-")
                ):
                    mapping[key], idx = _parse_yaml_block(lines, idx, next_indent)
                else:
                    mapping[key] = None
            else:
                mapping[key] = None
    return mapping, idx


def _consume_literal_block(
    lines: list[tuple[int, str]],
    start: int,
    min_indent: int,
) -> tuple[str, int]:
    collected: list[str] = []
    idx = start
    block_indent: int | None = None
    while idx < len(lines):
        indent, text = lines[idx]
        if text == "":
            collected.append("")
            idx += 1
            continue
        if indent < min_indent:
            break
        if block_indent is None:
            block_indent = indent
        pad = " " * max(0, indent - block_indent)
        collected.append(f"{pad}{text}")
        idx += 1
    return "\n".join(collected).strip("\n"), idx


def parse_yaml_documents(raw: str) -> list[Any]:
    """Parse one or more YAML documents (or JSON) using Python stdlib only."""
    stripped = raw.strip()
    if not stripped:
        return []
    if stripped.startswith(("{", "[")):
        try:
            parsed = json.loads(stripped)
            return [parsed]
        except json.JSONDecodeError:
            pass

    raw_docs = re.split(r"(?m)^---\s*$", raw)
    docs: list[Any] = []
    for chunk in raw_docs:
        if not chunk.strip():
            continue
        prepared: list[tuple[int, str]] = []
        in_literal = False
        literal_indent = 0
        for raw_line in chunk.splitlines():
            expanded = raw_line.replace("\t", "  ")
            if not expanded.strip():
                if in_literal:
                    prepared.append((literal_indent, ""))
                continue
            indent = len(expanded) - len(expanded.lstrip(" "))
            if in_literal and indent >= literal_indent:
                prepared.append((indent, expanded.strip()))
                continue
            in_literal = False
            cleaned = _strip_yaml_comment(expanded)
            if not cleaned.strip():
                continue
            text = cleaned.strip()
            prepared.append((indent, text))
            kv = _split_yaml_key_val(text)
            if kv is not None and kv[1] in ("|", "|-", ">", ">-"):
                in_literal = True
                literal_indent = indent + 1
        if not prepared:
            continue
        doc, _ = _parse_yaml_block(prepared, 0, 0)
        if doc is not None:
            docs.append(doc)
    return docs


# ----------------------------------------------------------------------
# Migration data structures
# ----------------------------------------------------------------------


@dataclass(slots=True)
class ParityGap:
    """Exact parity gap record comparing upstream vLLM/llm-d feature vs `fw-cluster`."""

    feature: str
    upstream_value: str
    fw_status: str  # "mapped" | "partial" | "unsupported_in_fw"
    recommendation: str


@dataclass(slots=True)
class MigrationResult:
    """Output of migrating a vLLM or llm-d deployment into Fw `fw-cluster.json`."""

    source_kind: str
    source_path: str | None
    target_dir: str
    applied: bool
    cluster_config: dict[str, Any]
    parity_gaps: list[ParityGap] = field(default_factory=list)
    written_files: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_kind": self.source_kind,
            "source_path": self.source_path,
            "target_dir": self.target_dir,
            "applied": self.applied,
            "written_files": list(self.written_files),
            "parity_summary": {
                "total": len(self.parity_gaps),
                "mapped": sum(1 for g in self.parity_gaps if g.fw_status == "mapped"),
                "partial": sum(1 for g in self.parity_gaps if g.fw_status == "partial"),
                "unsupported_in_fw": sum(
                    1 for g in self.parity_gaps if g.fw_status == "unsupported_in_fw"
                ),
            },
            "parity_gaps": [asdict(g) for g in self.parity_gaps],
            "cluster_config": self.cluster_config,
        }


def _make_cluster_node(
    *,
    node_id: str,
    endpoint: str,
    kind: str = "GpuHost",
    rpc_addr: str | None = None,
    gpu_model: str = "NVIDIA GPU (migrated)",
    vram_gb: int = 24,
    ram_gb: int = 64,
    cpu_cores: int = 16,
    prefill_score: float = 1.0,
    decode_score: float = 1.0,
) -> dict[str, Any]:
    """Build a `ClusterNode` dict matching `crates/fw-cluster/src/lib.rs`."""
    return {
        "id": node_id,
        "kind": kind,
        "endpoint": endpoint,
        "rpc_addr": rpc_addr,
        "gpu_model": gpu_model,
        "vram_gb": vram_gb,
        "ram_gb": ram_gb,
        "cpu_cores": cpu_cores,
        "healthy": True,
        "inflight": 0,
        "prefill_score": round(prefill_score, 3),
        "decode_score": round(decode_score, 3),
        "cached_prefixes": [],
    }


def _default_vps_proxy_node() -> dict[str, Any]:
    return _make_cluster_node(
        node_id="vps-1-pingora-ingress",
        endpoint="http://127.0.0.1:8080",
        kind="VpsProxyLbStorageCpu",
        rpc_addr="127.0.0.1:50052",
        gpu_model="none (Pingora Proxy/LB CPU)",
        vram_gb=0,
        ram_gb=32,
        cpu_cores=8,
        prefill_score=0.5,
        decode_score=0.6,
    )


# ----------------------------------------------------------------------
# vLLM migration (CLI flags / vllm_config.json / docker-compose)
# ----------------------------------------------------------------------


def parse_vllm_cli_flags(raw_cmd: str) -> dict[str, Any]:
    """Parse vLLM CLI flags (e.g. `vllm serve <model> --port 8000 ...`) into a dict."""
    cleaned = raw_cmd.replace("\\\n", " ").replace("\\\r\n", " ")
    tokens = shlex.split(cleaned)
    parsed: dict[str, Any] = {}
    positional: list[str] = []

    idx = 0
    while idx < len(tokens):
        tok = tokens[idx]
        if tok in ("vllm", "serve", "python", "python3", "-m", "vllm.entrypoints.openai.api_server"):
            idx += 1
            continue
        if tok.startswith("--"):
            if "=" in tok:
                flag, val = tok[2:].split("=", 1)
                parsed[flag.replace("-", "_")] = _parse_yaml_scalar(val)
                idx += 1
            else:
                flag = tok[2:].replace("-", "_")
                if idx + 1 < len(tokens) and not tokens[idx + 1].startswith("--"):
                    parsed[flag] = _parse_yaml_scalar(tokens[idx + 1])
                    idx += 2
                else:
                    parsed[flag] = True
                    idx += 1
        elif tok.startswith("-"):
            idx += 1
        else:
            positional.append(tok)
            idx += 1

    if "model" not in parsed and positional:
        parsed["model"] = positional[0]
    return parsed


def _extract_vllm_services_from_compose(doc: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    services = doc.get("services")
    if not isinstance(services, dict):
        return []
    extracted: list[tuple[str, dict[str, Any]]] = []
    for svc_name, svc_cfg in services.items():
        if not isinstance(svc_cfg, dict):
            continue
        image = str(svc_cfg.get("image", ""))
        cmd = svc_cfg.get("command")
        cmd_str = " ".join(str(c) for c in cmd) if isinstance(cmd, list) else str(cmd or "")
        if "vllm" not in image.lower() and "vllm" not in svc_name.lower() and "--model" not in cmd_str:
            continue
        flags = parse_vllm_cli_flags(cmd_str)
        ports = svc_cfg.get("ports") or []
        if "port" not in flags and isinstance(ports, list) and ports:
            first_port = str(ports[0]).split(":")[0]
            if first_port.isdigit():
                flags["port"] = int(first_port)
        flags.setdefault("host", svc_name)
        extracted.append((svc_name, flags))
    return extracted


def migrate_vllm_to_fw(
    raw_content: str | None = None,
    *,
    source_path: str | None = None,
    target_dir: str = ".",
    apply: bool = False,
) -> MigrationResult:
    """Convert vLLM CLI flags, `vllm_config.json`, or `docker-compose` into `fw-cluster.json`."""
    services: list[tuple[str, dict[str, Any]]] = []
    input_format = "default"

    if raw_content is not None and raw_content.strip():
        stripped = raw_content.strip()
        if stripped.startswith("{"):
            cfg = json.loads(stripped)
            input_format = "vllm_config.json"
            if isinstance(cfg.get("services"), list):
                for idx, svc in enumerate(cfg["services"]):
                    services.append((str(svc.get("id", f"vllm-node-{idx + 1}")), dict(svc)))
            else:
                services.append((str(cfg.get("node_id", "vllm-node-1")), cfg))
        elif "services:" in stripped:
            docs = parse_yaml_documents(stripped)
            if docs and isinstance(docs[0], dict):
                services = _extract_vllm_services_from_compose(docs[0])
                input_format = "docker-compose"
        if not services:
            input_format = "vllm-cli"
            services.append(("vllm-node-1", parse_vllm_cli_flags(stripped)))
    else:
        services.append(
            (
                "vllm-node-1",
                {
                    "model": "Qwen/Qwen3-32B",
                    "host": "127.0.0.1",
                    "port": 8000,
                    "tensor_parallel_size": 1,
                    "enable_prefix_caching": True,
                },
            )
        )

    nodes: list[dict[str, Any]] = []
    gaps: list[ParityGap] = []
    models: list[str] = []
    kv_connectors: list[dict[str, Any]] = []

    for idx, (svc_name, cfg) in enumerate(services, start=1):
        model = str(cfg.get("model", "unknown-model"))
        if model not in models:
            models.append(model)
        host = str(cfg.get("host", "127.0.0.1"))
        if host in ("0.0.0.0", "::"):
            host = "127.0.0.1"
        port = int(cfg.get("port", 8000 + idx - 1))
        tp = int(cfg.get("tensor_parallel_size", 1))
        pp = int(cfg.get("pipeline_parallel_size", 1))
        dp = int(cfg.get("data_parallel_size", 1))

        kv_cfg_raw = cfg.get("kv_transfer_config")
        kv_cfg: dict[str, Any] = {}
        if isinstance(kv_cfg_raw, str):
            try:
                kv_cfg = json.loads(kv_cfg_raw)
            except json.JSONDecodeError:
                kv_cfg = {"raw": kv_cfg_raw}
        elif isinstance(kv_cfg_raw, dict):
            kv_cfg = kv_cfg_raw

        kv_role = str(kv_cfg.get("kv_role", "kv_both"))
        prefill_score = 1.4 * tp if kv_role == "kv_producer" else 1.0 * tp
        decode_score = 1.4 * tp if kv_role == "kv_consumer" else 1.0 * tp
        if kv_role == "kv_producer":
            decode_score = 0.2
        elif kv_role == "kv_consumer":
            prefill_score = 0.2

        nodes.append(
            _make_cluster_node(
                node_id=svc_name,
                endpoint=f"http://{host}:{port}",
                kind="BaseCudaPc" if host == "127.0.0.1" else "GpuHost",
                rpc_addr=f"{host}:{50051 + idx}",
                gpu_model=f"vLLM GPU (TP={tp}, PP={pp}, DP={dp})",
                vram_gb=24 * max(1, tp),
                ram_gb=64,
                cpu_cores=16,
                prefill_score=prefill_score,
                decode_score=decode_score,
            )
        )

        if kv_cfg:
            kv_connectors.append({"node_id": svc_name, **kv_cfg})
            connector_name = str(kv_cfg.get("kv_connector", "unknown"))
            gaps.append(
                ParityGap(
                    feature="kv_transfer_config",
                    upstream_value=f"{connector_name} ({kv_role})",
                    fw_status="partial",
                    recommendation=(
                        "Wire plugins/vllm-fw/fw_vllm_plugin.py (FwKVConnectorV1) to bridge "
                        f"{connector_name} blocks into fw-cluster prefix index; RDMA/UCX direct "
                        "memory transfer stays inside vLLM worker processes."
                    ),
                )
            )
        if cfg.get("enable_prefix_caching"):
            gaps.append(
                ParityGap(
                    feature="enable_prefix_caching",
                    upstream_value="true",
                    fw_status="mapped",
                    recommendation=(
                        "Mapped to fw-cluster HybridClusterRouter.kv_prefix_index "
                        "(SHA-256 128-byte 16-hex prefix key)."
                    ),
                )
            )
        if cfg.get("kv_events_config"):
            gaps.append(
                ParityGap(
                    feature="kv_events_config",
                    upstream_value=str(cfg["kv_events_config"]),
                    fw_status="partial",
                    recommendation=(
                        "vLLM ZMQ KV-events pub/replay sockets (5556/5559) require an event "
                        "bridge adapter to push BlockStored/BlockRemoved updates into fw-cluster."
                    ),
                )
            )
        if cfg.get("enable_lora"):
            gaps.append(
                ParityGap(
                    feature="enable_lora",
                    upstream_value="true",
                    fw_status="unsupported_in_fw",
                    recommendation=(
                        "Dynamic multi-LoRA adapter-aware routing is not yet modeled in "
                        "fw-cluster ClusterNode; pin LoRA pools per node_id or add adapter tags."
                    ),
                )
            )
        if pp > 1 or dp > 1:
            gaps.append(
                ParityGap(
                    feature="parallelism_pp_dp",
                    upstream_value=f"PP={pp}, DP={dp}",
                    fw_status="partial",
                    recommendation=(
                        "fw-cluster routes at endpoint granularity; expose each DP rank as a "
                        "distinct ClusterNode if independent prefix caching is desired."
                    ),
                )
            )

    nodes.append(_default_vps_proxy_node())
    if not gaps:
        gaps.append(
            ParityGap(
                feature="vllm_basic_serving",
                upstream_value=f"{input_format} ({len(services)} node(s))",
                fw_status="mapped",
                recommendation="All core serving endpoints mapped cleanly to fw-cluster.",
            )
        )

    cluster_config: dict[str, Any] = {
        "cluster_name": "fw-migrated-vllm",
        "nodes": nodes,
        "kv_prefix_index": {},
        "migration": {
            "source_kind": "vllm",
            "input_format": input_format,
            "models": models,
            "kv_connectors": kv_connectors,
        },
    }
    return _finalize_migration("vllm", source_path, target_dir, apply, cluster_config, gaps)


# ----------------------------------------------------------------------
# llm-d migration (Helm values.yaml / InferencePool / EndpointPickerConfig)
# ----------------------------------------------------------------------


def migrate_llmd_to_fw(
    raw_content: str | None = None,
    *,
    source_path: str | None = None,
    target_dir: str = ".",
    apply: bool = False,
) -> MigrationResult:
    """Convert `llm-d` Helm values or `InferencePool` YAML/JSON into `fw-cluster.json`."""
    docs = parse_yaml_documents(raw_content) if raw_content and raw_content.strip() else []
    epp_configs: list[dict[str, Any]] = []
    inference_pools: list[dict[str, Any]] = []
    router_blocks: list[dict[str, Any]] = []

    for doc in docs:
        if not isinstance(doc, dict):
            continue
        kind = str(doc.get("kind", ""))
        if kind == "EndpointPickerConfig":
            epp_configs.append(doc)
        elif kind == "InferencePool":
            inference_pools.append(doc)
        if isinstance(doc.get("router"), dict):
            router_cfg = doc["router"]
            router_blocks.append(router_cfg)
            epp_section = router_cfg.get("epp")
            if isinstance(epp_section, dict):
                custom = epp_section.get("pluginsCustomConfig")
                if isinstance(custom, dict):
                    for embedded_yaml in custom.values():
                        if isinstance(embedded_yaml, str):
                            for sub_doc in parse_yaml_documents(embedded_yaml):
                                if isinstance(sub_doc, dict):
                                    epp_configs.append(sub_doc)

    nodes: list[dict[str, Any]] = []
    gaps: list[ParityGap] = []
    plugin_types: list[str] = []
    profile_names: list[str] = []
    model_name = "unknown-model"
    target_port = 8000
    is_disaggregated = False
    peak_prefill_tp: float | None = None

    for epp in epp_configs:
        for plugin in epp.get("plugins") or []:
            if not isinstance(plugin, dict):
                continue
            ptype = str(plugin.get("type", ""))
            if ptype:
                plugin_types.append(ptype)
            params = plugin.get("parameters") or {}
            if isinstance(params, dict):
                if "modelName" in params:
                    model_name = str(params["modelName"])
                if "peakPrefillThroughput" in params:
                    peak_prefill_tp = float(params["peakPrefillThroughput"])
                if "kvEventsConfig" in params:
                    gaps.append(
                        ParityGap(
                            feature="precise-prefix-cache-producer.kvEventsConfig",
                            upstream_value=str(params["kvEventsConfig"]),
                            fw_status="partial",
                            recommendation=(
                                "ZMQ KV-event pod discovery (ports 5556/5559) maps to "
                                "fw-cluster kv_prefix_index via plugins/vllm-fw/fw_vllm_plugin.py."
                            ),
                        )
                    )
        for profile in epp.get("schedulingProfiles") or []:
            if isinstance(profile, dict) and profile.get("name"):
                profile_names.append(str(profile["name"]))

    if "prefill" in profile_names and "decode" in profile_names:
        is_disaggregated = True
    if any("disagg" in p for p in plugin_types):
        is_disaggregated = True

    for pool in inference_pools:
        spec = pool.get("spec") or {}
        if isinstance(spec, dict):
            ports = spec.get("targetPorts") or []
            if isinstance(ports, list) and ports and isinstance(ports[0], dict):
                target_port = int(ports[0].get("number", target_port))
            elif "targetPortNumber" in spec:
                target_port = int(spec["targetPortNumber"])
        gaps.append(
            ParityGap(
                feature="InferencePool (Gateway API Inference Extension)",
                upstream_value=str(pool.get("apiVersion", "inference.networking.k8s.io/v1")),
                fw_status="partial",
                recommendation=(
                    "K8s Envoy ext-proc InferencePool is replaced by Pingora consistent-hash "
                    "ingress + plugins/llmd-fw/fw_llmd_scorer.py in fw-cluster."
                ),
            )
        )

    for rblock in router_blocks:
        ms = rblock.get("modelServers") or {}
        if isinstance(ms, dict):
            ports = ms.get("targetPorts") or []
            if isinstance(ports, list) and ports and isinstance(ports[0], dict):
                target_port = int(ports[0].get("number", target_port))
        epp_sec = rblock.get("epp") or {}
        if isinstance(epp_sec, dict) and int(epp_sec.get("replicas", 1)) > 1:
            gaps.append(
                ParityGap(
                    feature="epp.replicas > 1",
                    upstream_value=str(epp_sec.get("replicas")),
                    fw_status="partial",
                    recommendation=(
                        "Upstream llm-d notes active-active EPP replicas split local inflight "
                        "accounting; fw-cluster uses a single authoritative router state per VPS."
                    ),
                )
            )

    if is_disaggregated:
        nodes.append(
            _make_cluster_node(
                node_id="llmd-prefill-1",
                endpoint=f"http://10.66.0.21:{target_port}",
                kind="GpuHost",
                rpc_addr="10.66.0.21:50052",
                gpu_model=f"llm-d Prefill Pool ({model_name})",
                vram_gb=24,
                ram_gb=64,
                cpu_cores=16,
                prefill_score=1.6,
                decode_score=0.2,
            )
        )
        nodes.append(
            _make_cluster_node(
                node_id="llmd-decode-1",
                endpoint="http://10.66.0.22:8200",
                kind="GpuHost",
                rpc_addr="10.66.0.22:50052",
                gpu_model=f"llm-d Decode Pool ({model_name})",
                vram_gb=24,
                ram_gb=64,
                cpu_cores=16,
                prefill_score=0.2,
                decode_score=1.6,
            )
        )
        gaps.append(
            ParityGap(
                feature="pd-disaggregation (prefill/decode profiles)",
                upstream_value=", ".join(profile_names) or "prefill, decode",
                fw_status="mapped",
                recommendation=(
                    "Mapped to fw-cluster disaggregated prefill/decode nodes and "
                    "plugins/llmd-fw/fw_llmd_scorer.py::select_pd_endpoints."
                ),
            )
        )
    else:
        nodes.append(
            _make_cluster_node(
                node_id="llmd-worker-1",
                endpoint=f"http://10.66.0.21:{target_port}",
                kind="GpuHost",
                rpc_addr="10.66.0.21:50052",
                gpu_model=f"llm-d ModelServer ({model_name})",
                vram_gb=24,
                ram_gb=64,
                cpu_cores=16,
                prefill_score=1.2,
                decode_score=1.2,
            )
        )

    if "token-producer" in plugin_types:
        gaps.append(
            ParityGap(
                feature="token-producer (/v1/*/render)",
                upstream_value=f"modelName={model_name}",
                fw_status="unsupported_in_fw",
                recommendation=(
                    "fw-cluster hashes raw UTF-8 prompt prefixes (128 bytes) directly without "
                    "calling /v1/completions/render; keep token-producer only if exact token-ID "
                    "chunk boundaries are mandatory."
                ),
            )
        )

    if peak_prefill_tp is not None:
        gaps.append(
            ParityGap(
                feature="prefix-cache-affinity-filter.peakPrefillThroughput",
                upstream_value=str(peak_prefill_tp),
                fw_status="mapped",
                recommendation=(
                    "Mapped directly to ScorerWeights.peak_prefill_throughput in "
                    "plugins/llmd-fw/fw_llmd_scorer.py."
                ),
            )
        )

    nodes.append(_default_vps_proxy_node())

    cluster_config: dict[str, Any] = {
        "cluster_name": "fw-migrated-llmd",
        "nodes": nodes,
        "kv_prefix_index": {},
        "migration": {
            "source_kind": "llmd",
            "model": model_name,
            "disaggregated_pd": is_disaggregated,
            "epp_plugins": plugin_types,
            "scheduling_profiles": profile_names,
            "peak_prefill_throughput": peak_prefill_tp,
        },
    }
    return _finalize_migration("llmd", source_path, target_dir, apply, cluster_config, gaps)


def _finalize_migration(
    source_kind: str,
    source_path: str | None,
    target_dir: str,
    apply: bool,
    cluster_config: dict[str, Any],
    gaps: list[ParityGap],
) -> MigrationResult:
    written: list[str] = []
    if apply:
        out_dir = Path(target_dir)
        out_dir.mkdir(parents=True, exist_ok=True)
        cluster_file = out_dir / "fw-cluster.json"
        parity_file = out_dir / "fw-parity-report.json"
        cluster_file.write_text(
            json.dumps(cluster_config, indent=2) + "\n",
            encoding="utf-8",
        )
        parity_file.write_text(
            json.dumps([asdict(g) for g in gaps], indent=2) + "\n",
            encoding="utf-8",
        )
        written = [str(cluster_file), str(parity_file)]

    return MigrationResult(
        source_kind=source_kind,
        source_path=source_path,
        target_dir=target_dir,
        applied=apply,
        cluster_config=cluster_config,
        parity_gaps=gaps,
        written_files=written,
    )


def run_migration(
    mode: str,
    *,
    source: str | None = None,
    target: str = ".",
    apply: bool = False,
) -> MigrationResult:
    """Load `source` (file path or None) and execute `vllm` or `llmd` migration."""
    raw_content: str | None = None
    if source is not None:
        src_path = Path(source)
        if not src_path.exists():
            raise FileNotFoundError(f"Migration source not found: {src_path}")
        raw_content = src_path.read_text(encoding="utf-8")

    if mode == "vllm":
        return migrate_vllm_to_fw(
            raw_content,
            source_path=source,
            target_dir=target,
            apply=apply,
        )
    if mode == "llmd":
        return migrate_llmd_to_fw(
            raw_content,
            source_path=source,
            target_dir=target,
            apply=apply,
        )
    raise ValueError(f"Unsupported migration mode {mode!r}; expected 'vllm' or 'llmd'")


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="migrate_vllm_llmd_to_fw",
        description=(
            "Convert vLLM CLI/JSON/docker-compose or llm-d Helm/InferencePool configs "
            "into fw-cluster.json with exact parity gap report."
        ),
    )
    parser.add_argument(
        "mode",
        choices=("vllm", "llmd"),
        help="Source ecosystem to migrate from.",
    )
    parser.add_argument(
        "--source",
        default=None,
        help="Path to vLLM flags/JSON/compose file or llm-d Helm values/InferencePool YAML/JSON.",
    )
    parser.add_argument(
        "--target",
        default=".",
        help="Target output directory for fw-cluster.json (default: current directory).",
    )
    action_group = parser.add_mutually_exclusive_group()
    action_group.add_argument(
        "--dry-run",
        action="store_true",
        default=False,
        help="Preview fw-cluster.json and parity gaps without writing files (default).",
    )
    action_group.add_argument(
        "--apply",
        action="store_true",
        default=False,
        help="Write fw-cluster.json and fw-parity-report.json into --target.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)
    result = run_migration(
        args.mode,
        source=args.source,
        target=args.target,
        apply=bool(args.apply),
    )
    sys.stdout.write(json.dumps(result.to_dict(), indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
