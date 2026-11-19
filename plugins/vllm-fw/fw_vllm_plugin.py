"""Fairwind native vLLM general plugin & KVConnectorBase_V1 adapter.

Implements the upstream `vllm.general_plugins` entry-point protocol (`register()`)
and a stdlib-only `FairwindKVConnectorV1` mirror of vLLM's `KVConnectorBase_V1`
lifecycle (scheduler-side + worker-side), bridging vLLM PagedAttention/NIXL KV
blocks to Fairwind's `fw-cluster` radix prefix index and `fw bench`.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import time
from collections.abc import Sequence
from dataclasses import asdict, dataclass, field
from enum import Enum
from typing import Any

PLUGIN_GROUP = "vllm.general_plugins"
PLUGIN_NAME = "fairwind_vllm_kv"
DEFAULT_BLOCK_SIZE = 16
SUPPORTED_TRANSPORTS = ("nixl", "mooncake", "lmcache", "fw-rpc")


class KVRole(str, Enum):
    """vLLM `kv_transfer_config.kv_role` values for disaggregated P/D."""

    KV_PRODUCER = "kv_producer"
    KV_CONSUMER = "kv_consumer"
    KV_BOTH = "kv_both"


@dataclass(slots=True)
class KVConnectorConfig:
    """Configuration aligned with vLLM `KVTransferConfig` and `fw-cluster`."""

    kv_connector: str = "FairwindKVConnectorV1"
    kv_role: KVRole = KVRole.KV_BOTH
    block_size: int = DEFAULT_BLOCK_SIZE
    transport: str = "nixl"
    fw_cluster_endpoint: str = "http://127.0.0.1:4379"
    kv_events_port: int = 5556
    kv_replay_port: int = 5559
    node_id: str = "gpu-1-pc-rtx4070"

    def __post_init__(self) -> None:
        if isinstance(self.kv_role, str):
            self.kv_role = KVRole(self.kv_role)
        if self.block_size <= 0:
            raise ValueError(f"block_size must be positive, got {self.block_size}")
        if self.transport not in SUPPORTED_TRANSPORTS:
            raise ValueError(
                f"Unsupported transport {self.transport!r}; expected one of {SUPPORTED_TRANSPORTS}"
            )

    @classmethod
    def from_vllm_json(cls, raw: str | dict[str, Any]) -> KVConnectorConfig:
        """Build a connector config from vLLM `--kv-transfer-config` JSON."""
        data = json.loads(raw) if isinstance(raw, str) else dict(raw)
        extra = data.get("kv_connector_extra_config") or {}
        role_val = data.get("kv_role", KVRole.KV_BOTH.value)
        transport = str(extra.get("transport", "nixl")).lower()
        if transport not in SUPPORTED_TRANSPORTS:
            connector_name = str(data.get("kv_connector", "")).lower()
            if "mooncake" in connector_name:
                transport = "mooncake"
            elif "lmcache" in connector_name:
                transport = "lmcache"
            elif "nixl" in connector_name:
                transport = "nixl"
            else:
                transport = "fw-rpc"
        return cls(
            kv_connector=str(data.get("kv_connector", "FairwindKVConnectorV1")),
            kv_role=KVRole(role_val),
            block_size=int(extra.get("block_size", DEFAULT_BLOCK_SIZE)),
            transport=transport,
            fw_cluster_endpoint=str(
                extra.get("fw_cluster_endpoint", "http://127.0.0.1:4379")
            ),
            kv_events_port=int(extra.get("kv_events_port", 5556)),
            kv_replay_port=int(extra.get("kv_replay_port", 5559)),
            node_id=str(extra.get("node_id", "gpu-1-pc-rtx4070")),
        )


@dataclass(slots=True)
class KVBlockDescriptor:
    """Metadata describing a PagedAttention KV block registered for transfer."""

    block_hash: str
    prefix_key: str
    num_tokens: int
    layer_count: int
    owner_node: str
    transport: str
    nixl_reg_id: str | None = None
    saved_at: float = field(default_factory=time.time)


@dataclass(slots=True)
class KVTransferResult:
    """Outcome of a disaggregated P/D KV-cache transfer."""

    request_id: str
    matched_tokens: int
    transferred_blocks: list[str]
    source_node: str
    target_node: str
    transport: str
    status: str = "completed"


def fw_prefix_key(prompt: str) -> str:
    """Compute the 16-char lowercase hex prefix hash matching `fw-cluster::prefix_key`.

    Mirrors `crates/fw-cluster/src/lib.rs`:
    SHA-256 of the first 128 UTF-8 bytes of `prompt`, truncated to 8 bytes (16 hex chars).
    """
    prefix_bytes = prompt.encode("utf-8")[:128]
    return hashlib.sha256(prefix_bytes).hexdigest()[:16]


def compute_paged_block_hashes(
    token_ids: Sequence[int],
    block_size: int = DEFAULT_BLOCK_SIZE,
) -> list[str]:
    """Compute chained SHA-256 PagedAttention APC block hashes for full blocks."""
    if block_size <= 0:
        raise ValueError(f"block_size must be positive, got {block_size}")
    hashes: list[str] = []
    parent = b"fw-vllm-root"
    full_blocks = len(token_ids) // block_size
    for idx in range(full_blocks):
        chunk = token_ids[idx * block_size : (idx + 1) * block_size]
        payload = parent + b"|" + ",".join(str(t) for t in chunk).encode("ascii")
        digest = hashlib.sha256(payload).hexdigest()[:24]
        hashes.append(digest)
        parent = digest.encode("ascii")
    return hashes


class FairwindKVConnectorV1:
    """Stdlib-only vLLM `KVConnectorBase_V1` adapter bridged to `fw-cluster` & `fw bench`.

    Implements both scheduler-side and worker-side primitives defined in
    `vllm/distributed/kv_transfer/kv_connector/v1/base.py`:
    - Scheduler-side: `get_num_new_matched_tokens`, `update_state_after_alloc`,
      `request_finished`, `take_events`.
    - Worker-side: `handle_preemptions`, `start_load_kv`, `wait_for_layer_load`,
      `save_kv_layer`, `wait_for_save`, `get_transfer_results`,
      `build_connector_worker_meta`.
    """

    def __init__(self, config: KVConnectorConfig | None = None) -> None:
        self.config = config or KVConnectorConfig()
        self._blocks: dict[str, KVBlockDescriptor] = {}
        self._prefix_index: dict[str, str] = {}
        self._allocations: dict[str, tuple[list[str], int]] = {}
        self._pending_loads: dict[str, list[str]] = {}
        self._pending_saves: dict[str, dict[int, list[str]]] = {}
        self._done_sending: set[str] = set()
        self._done_recving: set[str] = set()
        self._events: list[dict[str, Any]] = []
        self._transfer_history: list[KVTransferResult] = []

    # ------------------------------------------------------------------
    # Scheduler-side primitives (vLLM KVConnectorBase_V1)
    # ------------------------------------------------------------------

    def get_num_new_matched_tokens(
        self,
        request_id: str,
        token_ids: Sequence[int],
        *,
        prompt: str = "",
    ) -> tuple[int, bool]:
        """Return `(num_external_matched_tokens, load_async)` without side effects."""
        del request_id
        block_hashes = compute_paged_block_hashes(token_ids, self.config.block_size)
        matched_blocks = 0
        for bh in block_hashes:
            if bh in self._blocks:
                matched_blocks += 1
            else:
                break
        matched_tokens = matched_blocks * self.config.block_size
        if matched_tokens == 0 and prompt:
            pkey = fw_prefix_key(prompt)
            if pkey in self._prefix_index:
                matched_tokens = min(len(token_ids), self.config.block_size)
        load_async = matched_tokens > 0 and self.config.kv_role in (
            KVRole.KV_CONSUMER,
            KVRole.KV_BOTH,
        )
        return matched_tokens, load_async

    def update_state_after_alloc(
        self,
        request_id: str,
        block_hashes: Sequence[str],
        num_external_tokens: int,
    ) -> None:
        """Record allocated block hashes after vLLM CacheManager allocation."""
        self._allocations[request_id] = (list(block_hashes), num_external_tokens)

    def request_finished(
        self,
        request_id: str,
        block_hashes: Sequence[str],
        *,
        prompt: str = "",
    ) -> tuple[bool, dict[str, Any] | None]:
        """Finalize request KV state and emit `fw-cluster` / NIXL transfer params.

        Returns `(delay_free_blocks, kv_transfer_params)`.
        """
        pkey = fw_prefix_key(prompt) if prompt else ""
        if pkey:
            self._prefix_index[pkey] = self.config.node_id

        for bh in block_hashes:
            if bh not in self._blocks:
                self._blocks[bh] = KVBlockDescriptor(
                    block_hash=bh,
                    prefix_key=pkey,
                    num_tokens=self.config.block_size,
                    layer_count=1,
                    owner_node=self.config.node_id,
                    transport=self.config.transport,
                    nixl_reg_id=f"{self.config.transport}:{self.config.node_id}:{bh[:8]}",
                )

        delay_free = self.config.kv_role == KVRole.KV_PRODUCER and len(block_hashes) > 0
        self._events.append(
            {
                "type": "BlockStored",
                "request_id": request_id,
                "node_id": self.config.node_id,
                "prefix_key": pkey,
                "block_hashes": list(block_hashes),
                "transport": self.config.transport,
                "timestamp": time.time(),
            }
        )
        transfer_params: dict[str, Any] = {
            "do_remote_prefill": self.config.kv_role == KVRole.KV_PRODUCER,
            "remote_host": self.config.fw_cluster_endpoint,
            "remote_node_id": self.config.node_id,
            "transport": self.config.transport,
            "prefix_key": pkey,
            "block_hashes": list(block_hashes),
        }
        self._allocations.pop(request_id, None)
        return delay_free, transfer_params

    def take_events(self) -> list[dict[str, Any]]:
        """Drain and return KV-cache events for `llm-d` / `fw-cluster` subscribers."""
        drained = list(self._events)
        self._events.clear()
        return drained

    # ------------------------------------------------------------------
    # Worker-side primitives (vLLM KVConnectorBase_V1)
    # ------------------------------------------------------------------

    def handle_preemptions(self, preempted_request_ids: Sequence[str]) -> int:
        """Evict pending transfer state for preempted requests before overwrite."""
        removed = 0
        for req_id in preempted_request_ids:
            existed = (
                req_id in self._pending_loads
                or req_id in self._pending_saves
                or req_id in self._allocations
            )
            self._pending_loads.pop(req_id, None)
            self._pending_saves.pop(req_id, None)
            self._allocations.pop(req_id, None)
            if existed:
                removed += 1
                self._events.append(
                    {
                        "type": "BlockRemoved",
                        "request_id": req_id,
                        "node_id": self.config.node_id,
                        "reason": "preempted",
                        "timestamp": time.time(),
                    }
                )
        return removed

    def start_load_kv(
        self,
        request_id: str,
        block_hashes: Sequence[str],
    ) -> KVTransferResult:
        """Initiate loading remote KV blocks into the local worker."""
        available = [bh for bh in block_hashes if bh in self._blocks]
        self._pending_loads[request_id] = available
        source_node = (
            self._blocks[available[0]].owner_node if available else self.config.node_id
        )
        self._done_recving.add(request_id)
        result = KVTransferResult(
            request_id=request_id,
            matched_tokens=len(available) * self.config.block_size,
            transferred_blocks=available,
            source_node=source_node,
            target_node=self.config.node_id,
            transport=self.config.transport,
            status="completed" if available else "miss",
        )
        self._transfer_history.append(result)
        return result

    def wait_for_layer_load(self, request_id: str, layer_idx: int) -> bool:
        """Verify that KV blocks for `layer_idx` have finished loading."""
        del layer_idx
        return request_id in self._pending_loads

    def save_kv_layer(
        self,
        request_id: str,
        layer_idx: int,
        block_hashes: Sequence[str],
        *,
        prompt: str = "",
    ) -> None:
        """Stage a layer's KV blocks for remote NIXL / `fw-cluster` publication."""
        self._pending_saves.setdefault(request_id, {})[layer_idx] = list(block_hashes)
        pkey = fw_prefix_key(prompt) if prompt else ""
        if pkey:
            self._prefix_index[pkey] = self.config.node_id
        for bh in block_hashes:
            existing = self._blocks.get(bh)
            if existing is None:
                self._blocks[bh] = KVBlockDescriptor(
                    block_hash=bh,
                    prefix_key=pkey,
                    num_tokens=self.config.block_size,
                    layer_count=layer_idx + 1,
                    owner_node=self.config.node_id,
                    transport=self.config.transport,
                    nixl_reg_id=f"{self.config.transport}:{self.config.node_id}:{bh[:8]}",
                )
            else:
                existing.layer_count = max(existing.layer_count, layer_idx + 1)
                if pkey and not existing.prefix_key:
                    existing.prefix_key = pkey

    def wait_for_save(self, request_id: str) -> int:
        """Finalize pending layer saves and mark `request_id` as done sending."""
        saved_layers = self._pending_saves.pop(request_id, {})
        if saved_layers:
            self._done_sending.add(request_id)
        return len(saved_layers)

    def get_transfer_results(self) -> tuple[set[str], set[str]]:
        """Snapshot and clear `(done_sending, done_recving)` request IDs."""
        sending = set(self._done_sending)
        recving = set(self._done_recving)
        self._done_sending.clear()
        self._done_recving.clear()
        return sending, recving

    def build_connector_worker_meta(self) -> dict[str, Any]:
        """Build worker-to-scheduler metadata snapshot."""
        return {
            "connector": self.config.kv_connector,
            "node_id": self.config.node_id,
            "role": self.config.kv_role.value,
            "transport": self.config.transport,
            "cached_blocks": len(self._blocks),
            "cached_prefixes": len(self._prefix_index),
            "pending_loads": len(self._pending_loads),
            "pending_saves": len(self._pending_saves),
        }

    # ------------------------------------------------------------------
    # Fairwind `fw-cluster` & `fw bench` bridge
    # ------------------------------------------------------------------

    def export_cluster_kv_index(self) -> dict[str, str]:
        """Export `prefix_hash -> node_id` compatible with `fw-cluster::HybridClusterRouter`."""
        return dict(self._prefix_index)

    def build_fw_bench_payload(
        self,
        model: str,
        prompt_tokens: int,
        decode_tokens: int,
    ) -> dict[str, Any]:
        """Build a structured benchmark specification for `fw bench`."""
        return {
            "tool": "fw bench",
            "adapter": PLUGIN_NAME,
            "model": model,
            "node_id": self.config.node_id,
            "cluster_endpoint": self.config.fw_cluster_endpoint,
            "kv_connector": self.config.kv_connector,
            "kv_role": self.config.kv_role.value,
            "transport": self.config.transport,
            "block_size": self.config.block_size,
            "prompt_tokens": prompt_tokens,
            "decode_tokens": decode_tokens,
            "registered_blocks": len(self._blocks),
            "registered_prefixes": len(self._prefix_index),
        }

    def run_fw_bench(
        self,
        *,
        model: str = "Qwen/Qwen3-32B",
        prompt_tokens: int = 1024,
        decode_tokens: int = 128,
        fw_bin: str | None = None,
        dry_run: bool = True,
    ) -> dict[str, Any]:
        """Invoke `fw bench` (or return a dry-run execution plan when `dry_run=True`)."""
        resolved_bin = fw_bin or shutil.which("fw") or "fw"
        cmd = [resolved_bin, "bench"]
        payload = self.build_fw_bench_payload(model, prompt_tokens, decode_tokens)
        if dry_run:
            return {
                "dry_run": True,
                "command": cmd,
                "payload": payload,
                "returncode": 0,
            }
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=False,
        )
        return {
            "dry_run": False,
            "command": cmd,
            "payload": payload,
            "returncode": proc.returncode,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
        }


# ----------------------------------------------------------------------
# vLLM `vllm.general_plugins` idempotent registration
# ----------------------------------------------------------------------

_REGISTERED: bool = False
_ACTIVE_CONNECTOR: FairwindKVConnectorV1 | None = None
_REGISTRATION_COUNT: int = 0


def _is_plugin_allowed_by_env(env: dict[str, str] | None = None) -> bool:
    """Honor vLLM's `VLLM_PLUGINS` comma-separated allowlist semantics.

    In `vllm/plugins/__init__.py`:
    - Unset (`None`): all general plugins are loaded.
    - Empty string (`""`): parses to `[""]`, matching no plugin name.
    - Non-empty string: plugin loads only if `PLUGIN_NAME` is listed.
    """
    source = os.environ if env is None else env
    raw = source.get("VLLM_PLUGINS")
    if raw is None:
        return True
    allowed = [item.strip() for item in raw.split(",")]
    return PLUGIN_NAME in allowed


def register(
    config: KVConnectorConfig | None = None,
    *,
    env: dict[str, str] | None = None,
) -> dict[str, Any]:
    """Entry point for `vllm.general_plugins` (`fw_vllm_plugin:register`).

    Idempotent across multiple calls and worker processes per vLLM's
    `load_general_plugins()` contract.
    """
    global _REGISTERED, _ACTIVE_CONNECTOR, _REGISTRATION_COUNT

    if not _is_plugin_allowed_by_env(env):
        return {
            "plugin": PLUGIN_NAME,
            "group": PLUGIN_GROUP,
            "registered": False,
            "skipped": True,
            "reason": "not in VLLM_PLUGINS allowlist",
        }

    if _REGISTERED and _ACTIVE_CONNECTOR is not None:
        return {
            "plugin": PLUGIN_NAME,
            "group": PLUGIN_GROUP,
            "registered": True,
            "already_registered": True,
            "registration_count": _REGISTRATION_COUNT,
            "connector": _ACTIVE_CONNECTOR.config.kv_connector,
        }

    _ACTIVE_CONNECTOR = FairwindKVConnectorV1(config=config)
    _REGISTERED = True
    _REGISTRATION_COUNT += 1
    return {
        "plugin": PLUGIN_NAME,
        "group": PLUGIN_GROUP,
        "registered": True,
        "already_registered": False,
        "registration_count": _REGISTRATION_COUNT,
        "connector": _ACTIVE_CONNECTOR.config.kv_connector,
        "config": asdict(_ACTIVE_CONNECTOR.config),
    }


def is_registered() -> bool:
    """Return True if `register()` has initialized the Fairwind vLLM plugin."""
    return _REGISTERED


def get_connector() -> FairwindKVConnectorV1 | None:
    """Return the active singleton `FairwindKVConnectorV1` instance, if registered."""
    return _ACTIVE_CONNECTOR


def reset_registry() -> None:
    """Reset module-level registration state (intended for unit tests)."""
    global _REGISTERED, _ACTIVE_CONNECTOR, _REGISTRATION_COUNT
    _REGISTERED = False
    _ACTIVE_CONNECTOR = None
    _REGISTRATION_COUNT = 0
