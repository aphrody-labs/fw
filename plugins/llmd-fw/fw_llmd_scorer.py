"""Fw native `llm-d` Endpoint Picker (EPP) scorer plugin.

Implements prefix-cache affinity, KV-cache utilization, queue/token-load scoring,
and disaggregated Prefill/Decode (P/D) endpoint selection aligned with:
- Upstream `llm-d` (`llm-d/llm-d`, `llm-d/llm-d-router` EPP `EndpointPickerConfig`
  plugins: `precise-prefix-cache-producer`, `prefix-cache-affinity-filter`,
  `token-load-scorer`, `active-request-scorer`, `disagg-profile-handler`)
- Fw `crates/fw-cluster/src/lib.rs` (`HybridClusterRouter`, `ClusterNode`,
  `RoutingDecision`, `prefix_key`).

Pure Python 3.12+ standard library only.
"""

from __future__ import annotations

import hashlib
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Any

EPP_API_VERSION = "llm-d.ai/v1"
EPP_KIND = "EndpointPickerConfig"
DEFAULT_PEAK_PREFILL_THROUGHPUT = 15926.0


def fw_prefix_key(prompt: str) -> str:
    """Compute the 16-char lowercase hex prefix hash matching `fw-cluster::prefix_key`."""
    prefix_bytes = prompt.encode("utf-8")[:128]
    return hashlib.sha256(prefix_bytes).hexdigest()[:16]


@dataclass(frozen=True, slots=True)
class EndpointState:
    """Unified endpoint state compatible with both `llm-d` InferencePool pods and `fw-cluster`."""

    id: str
    endpoint: str
    role: str = "both"  # "prefill" | "decode" | "both" | "proxy" | "webgpu_draft"
    kind: str = "GpuHost"
    healthy: bool = True
    inflight: int = 0
    queued_prefill_tokens: int = 0
    kv_cache_usage: float = 0.0
    prefill_score: float = 1.0
    decode_score: float = 1.0
    cached_prefixes: tuple[str, ...] = ()
    vram_gb: int = 0

    @classmethod
    def from_mapping(cls, raw: Mapping[str, Any]) -> EndpointState:
        """Coerce a `fw-cluster` `ClusterNode` or `llm-d` endpoint dict into `EndpointState`."""
        kind = str(raw.get("kind", "GpuHost"))
        default_role = "both"
        if kind == "UserWebGpuFpu":
            default_role = "webgpu_draft"
        elif kind == "VpsProxyLbStorageCpu" and raw.get("role") is None:
            default_role = "both"
        prefixes = raw.get("cached_prefixes") or ()
        return cls(
            id=str(raw["id"]),
            endpoint=str(raw.get("endpoint", "")),
            role=str(raw.get("role", default_role)),
            kind=kind,
            healthy=bool(raw.get("healthy", True)),
            inflight=max(0, int(raw.get("inflight", 0))),
            queued_prefill_tokens=max(0, int(raw.get("queued_prefill_tokens", 0))),
            kv_cache_usage=float(raw.get("kv_cache_usage", 0.0)),
            prefill_score=float(raw.get("prefill_score", 1.0)),
            decode_score=float(raw.get("decode_score", 1.0)),
            cached_prefixes=tuple(str(p) for p in prefixes),
            vram_gb=max(0, int(raw.get("vram_gb", 0))),
        )


@dataclass(frozen=True, slots=True)
class ScorerWeights:
    """Configurable EPP composite scorer weights and calibration constants."""

    prefix_affinity_weight: float = 0.45
    kv_load_weight: float = 0.25
    queue_depth_weight: float = 0.20
    hardware_score_weight: float = 0.10
    peak_prefill_throughput: float = DEFAULT_PEAK_PREFILL_THROUGHPUT


@dataclass(frozen=True, slots=True)
class ScoredEndpoint:
    """Per-endpoint scoring breakdown emitted by `score_endpoints()`."""

    endpoint_id: str
    endpoint: str
    role: str
    total_score: float
    prefix_score: float
    kv_load_score: float
    queue_score: float
    hardware_score: float
    prefix_hit: bool
    eligible: bool
    reason: str


@dataclass(frozen=True, slots=True)
class PdSelectionDecision:
    """Disaggregated Prefill/Decode routing decision aligned with `fw-cluster::RoutingDecision`."""

    prefix_hash: str
    kv_cache_hit: bool
    proxy_vps: str
    prefill_node: str
    decode_node: str
    draft_user_webgpu: str | None
    disaggregated: bool
    strategy: str
    prefill_scores: tuple[ScoredEndpoint, ...]
    decode_scores: tuple[ScoredEndpoint, ...]


def _coerce_endpoint(item: EndpointState | Mapping[str, Any]) -> EndpointState:
    if isinstance(item, EndpointState):
        return item
    return EndpointState.from_mapping(item)


def _clamp01(value: float) -> float:
    return max(0.0, min(1.0, value))


def _role_matches(endpoint: EndpointState, target_role: str) -> bool:
    if target_role == "any":
        return endpoint.role != "webgpu_draft"
    if endpoint.role == "both":
        return target_role in ("prefill", "decode")
    return endpoint.role == target_role


def _compute_prefix_affinity(
    prompt: str,
    prefix_hash: str,
    cached_prefixes: tuple[str, ...],
) -> tuple[float, bool]:
    """Return `(prefix_score_0_to_1, exact_hit)` for an endpoint's cached prefixes."""
    if not cached_prefixes:
        return 0.0, False
    if prefix_hash in cached_prefixes:
        return 1.0, True

    best_overlap = 0.0
    prompt_prefix = prompt[:128]
    if not prompt_prefix:
        return 0.0, False

    for cached in cached_prefixes:
        if cached == prompt_prefix or (
            len(cached) == 16 and cached == prefix_hash
        ):
            return 1.0, True
        if prompt.startswith(cached) and cached:
            ratio = min(1.0, len(cached) / max(len(prompt_prefix), 1))
            best_overlap = max(best_overlap, ratio)
        elif cached.startswith(prompt_prefix):
            best_overlap = max(best_overlap, 1.0)
        else:
            # Longest common character prefix ratio for partial prompt overlap
            limit = min(len(prompt_prefix), len(cached))
            common = 0
            while common < limit and prompt_prefix[common] == cached[common]:
                common += 1
            if common >= 16:
                best_overlap = max(best_overlap, common / len(prompt_prefix))
    return _clamp01(best_overlap), best_overlap >= 0.999


def score_endpoints(
    prompt: str,
    endpoints: Sequence[EndpointState | Mapping[str, Any]],
    *,
    role: str = "prefill",
    weights: ScorerWeights | None = None,
    exclude_ids: set[str] | None = None,
) -> list[ScoredEndpoint]:
    """Score candidate endpoints using `llm-d` EPP + `fw-cluster` composite scoring.

    Combines:
    1. Prefix-cache affinity (`fw-cluster` 16-hex SHA-256 prefix hash + partial overlap)
    2. KV-cache utilization headroom (`1 - kv_cache_usage`)
    3. Queue depth & in-flight prefill token load (`token-load-scorer` + `active-request-scorer`)
    4. Normalized node hardware score (`prefill_score` / `decode_score`)

    Returns eligible endpoints first sorted descending by `total_score`, followed by
    ineligible endpoints with score `0.0`.
    """
    cfg = weights or ScorerWeights()
    excluded = exclude_ids or set()
    coerced = [_coerce_endpoint(ep) for ep in endpoints]
    pkey = fw_prefix_key(prompt)

    max_hw = max(
        (
            (ep.prefill_score if role == "prefill" else ep.decode_score)
            for ep in coerced
            if ep.healthy
        ),
        default=1.0,
    )
    if max_hw <= 0.0:
        max_hw = 1.0

    scored: list[ScoredEndpoint] = []
    for ep in coerced:
        if not ep.healthy:
            scored.append(
                ScoredEndpoint(
                    endpoint_id=ep.id,
                    endpoint=ep.endpoint,
                    role=ep.role,
                    total_score=0.0,
                    prefix_score=0.0,
                    kv_load_score=0.0,
                    queue_score=0.0,
                    hardware_score=0.0,
                    prefix_hit=False,
                    eligible=False,
                    reason="unhealthy",
                )
            )
            continue
        if ep.id in excluded:
            scored.append(
                ScoredEndpoint(
                    endpoint_id=ep.id,
                    endpoint=ep.endpoint,
                    role=ep.role,
                    total_score=0.0,
                    prefix_score=0.0,
                    kv_load_score=0.0,
                    queue_score=0.0,
                    hardware_score=0.0,
                    prefix_hit=False,
                    eligible=False,
                    reason="excluded",
                )
            )
            continue
        if not _role_matches(ep, role):
            scored.append(
                ScoredEndpoint(
                    endpoint_id=ep.id,
                    endpoint=ep.endpoint,
                    role=ep.role,
                    total_score=0.0,
                    prefix_score=0.0,
                    kv_load_score=0.0,
                    queue_score=0.0,
                    hardware_score=0.0,
                    prefix_hit=False,
                    eligible=False,
                    reason=f"role_mismatch:{ep.role}!={role}",
                )
            )
            continue

        prefix_score, prefix_hit = _compute_prefix_affinity(
            prompt, pkey, ep.cached_prefixes
        )
        kv_load_score = 1.0 - _clamp01(ep.kv_cache_usage)
        token_load_penalty = (
            ep.queued_prefill_tokens / max(cfg.peak_prefill_throughput, 1.0)
            if role == "prefill"
            else 0.0
        )
        queue_score = 1.0 / (1.0 + float(ep.inflight) + token_load_penalty)
        raw_hw = ep.prefill_score if role == "prefill" else ep.decode_score
        hardware_score = _clamp01(raw_hw / max_hw)

        if role == "decode":
            # Pure decode is bound by concurrent active requests and KV headroom,
            # matching llm-d `decode` schedulingProfile (`active-request-scorer`).
            total = (
                0.10 * prefix_score
                + 0.35 * kv_load_score
                + 0.40 * queue_score
                + 0.15 * hardware_score
            )
        else:
            total = (
                cfg.prefix_affinity_weight * prefix_score
                + cfg.kv_load_weight * kv_load_score
                + cfg.queue_depth_weight * queue_score
                + cfg.hardware_score_weight * hardware_score
            )

        scored.append(
            ScoredEndpoint(
                endpoint_id=ep.id,
                endpoint=ep.endpoint,
                role=ep.role,
                total_score=round(_clamp01(total), 6),
                prefix_score=round(prefix_score, 6),
                kv_load_score=round(kv_load_score, 6),
                queue_score=round(queue_score, 6),
                hardware_score=round(hardware_score, 6),
                prefix_hit=prefix_hit,
                eligible=True,
                reason="ok",
            )
        )

    scored.sort(key=lambda item: (not item.eligible, -item.total_score, item.endpoint_id))
    return scored


def select_pd_endpoints(
    prompt: str,
    endpoints: Sequence[EndpointState | Mapping[str, Any]],
    *,
    prefer_user_webgpu_draft: bool = False,
    weights: ScorerWeights | None = None,
) -> PdSelectionDecision:
    """Select Pingora VPS ingress, Prefill node, Decode node, and optional WebGPU draft peer."""
    coerced = [_coerce_endpoint(ep) for ep in endpoints]
    pkey = fw_prefix_key(prompt)

    # 1. Pingora consistent-hash VPS proxy selection (parity with fw-cluster)
    vps_nodes = [
        ep
        for ep in coerced
        if ep.healthy and (ep.kind == "VpsProxyLbStorageCpu" or ep.role == "proxy")
    ]
    if vps_nodes:
        proxy_vps = vps_nodes[int(pkey, 16) % len(vps_nodes)].id
    else:
        proxy_vps = "local-direct"

    # 2. Score and pick best Prefill endpoint
    prefill_scores = score_endpoints(prompt, coerced, role="prefill", weights=weights)
    eligible_prefill = [s for s in prefill_scores if s.eligible]
    if not eligible_prefill:
        raise ValueError("No healthy prefill-capable endpoints available")
    best_prefill = eligible_prefill[0]

    # 3. Score and pick best distinct Decode endpoint for disaggregated P/D
    decode_scores = score_endpoints(
        prompt,
        coerced,
        role="decode",
        weights=weights,
        exclude_ids={best_prefill.endpoint_id},
    )
    eligible_decode = [s for s in decode_scores if s.eligible]
    if eligible_decode:
        best_decode = eligible_decode[0]
    else:
        # Fallback to co-located prefill+decode on the same endpoint
        fallback_decode = score_endpoints(
            prompt, coerced, role="decode", weights=weights
        )
        eligible_fallback = [s for s in fallback_decode if s.eligible]
        best_decode = eligible_fallback[0] if eligible_fallback else best_prefill
        decode_scores = fallback_decode

    # 4. Optional End-User Browser WebGPU/FPU speculative draft peer
    draft_peer: str | None = None
    if prefer_user_webgpu_draft:
        for ep in coerced:
            if ep.healthy and (
                ep.kind == "UserWebGpuFpu" or ep.role == "webgpu_draft"
            ):
                draft_peer = ep.id
                break

    disaggregated = best_prefill.endpoint_id != best_decode.endpoint_id
    return PdSelectionDecision(
        prefix_hash=pkey,
        kv_cache_hit=best_prefill.prefix_hit,
        proxy_vps=proxy_vps,
        prefill_node=best_prefill.endpoint_id,
        decode_node=best_decode.endpoint_id,
        draft_user_webgpu=draft_peer,
        disaggregated=disaggregated,
        strategy="llmd-epp-prefix-affinity + token-load + fw-cluster-pd",
        prefill_scores=tuple(prefill_scores),
        decode_scores=tuple(decode_scores),
    )
