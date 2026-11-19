"""vLLM PagedAttention KV allocator, Automatic Prefix Caching, CoW fork, and llm-d scheduler.

Implements:
- ``PhysicalBlockAllocator`` + ``BlockTable`` (16-token blocks by default)
- Automatic Prefix Caching (APC) via chained SHA-256 block hashes (``parent_hash + block_tokens``)
- Copy-on-Write (``fork_sequence``) for beam search / multi-subagent branching with refcounts
- ``LlmdDisaggregatedScheduler`` routing Prefill vs Decode workers by prefix hit rate and
  KV memory watermark, with structured preemption telemetry.
"""

from __future__ import annotations

import hashlib
import struct
from collections import OrderedDict
from collections.abc import Sequence
from dataclasses import dataclass, field
from enum import Enum

DEFAULT_BLOCK_SIZE: int = 16


def compute_block_hash(parent_hash: str | None, block_tokens: Sequence[int]) -> str:
    """Compute a chained SHA-256 prefix hash for a full KV block."""
    hasher = hashlib.sha256()
    if parent_hash is not None:
        hasher.update(parent_hash.encode("ascii"))
    hasher.update(b"|")
    if block_tokens:
        hasher.update(struct.pack(f"<{len(block_tokens)}q", *(int(t) for t in block_tokens)))
    return hasher.hexdigest()


@dataclass(slots=True)
class PhysicalBlock:
    """One physical KV cache block managed by ``PhysicalBlockAllocator``."""

    block_id: int
    block_size: int = DEFAULT_BLOCK_SIZE
    refcount: int = 0
    prefix_hash: str | None = None
    tokens: list[int] = field(default_factory=list)

    @property
    def is_full(self) -> bool:
        return len(self.tokens) == self.block_size

    @property
    def num_tokens(self) -> int:
        return len(self.tokens)


@dataclass(slots=True)
class BlockTable:
    """Logical-to-physical block mapping for a single sequence."""

    seq_id: str
    block_size: int = DEFAULT_BLOCK_SIZE
    physical_block_ids: list[int] = field(default_factory=list)
    token_ids: list[int] = field(default_factory=list)

    @property
    def num_blocks(self) -> int:
        return len(self.physical_block_ids)

    @property
    def num_tokens(self) -> int:
        return len(self.token_ids)


class OutOfKVBlocksError(MemoryError):
    """Raised when no free or evictable physical KV blocks remain."""


class PhysicalBlockAllocator:
    """vLLM-style PagedAttention physical KV block allocator with APC and CoW."""

    def __init__(self, num_blocks: int, block_size: int = DEFAULT_BLOCK_SIZE) -> None:
        if num_blocks <= 0:
            raise ValueError("num_blocks must be positive")
        if block_size <= 0:
            raise ValueError("block_size must be positive")
        self.num_blocks = num_blocks
        self.block_size = block_size
        self._blocks: list[PhysicalBlock] = [
            PhysicalBlock(block_id=i, block_size=block_size) for i in range(num_blocks)
        ]
        self._free_ids: list[int] = list(range(num_blocks - 1, -1, -1))
        self._prefix_index: dict[str, int] = {}
        self._evictable_lru: OrderedDict[int, str] = OrderedDict()
        self._tables: dict[str, BlockTable] = {}

        # Telemetry counters
        self.prefix_cache_hits: int = 0
        self.prefix_cache_misses: int = 0
        self.cow_copies: int = 0
        self.evictions: int = 0

    @property
    def active_block_count(self) -> int:
        return sum(1 for b in self._blocks if b.refcount > 0)

    @property
    def free_block_count(self) -> int:
        return len(self._free_ids)

    @property
    def evictable_cached_count(self) -> int:
        return len(self._evictable_lru)

    @property
    def available_block_count(self) -> int:
        return len(self._free_ids) + len(self._evictable_lru)

    @property
    def kv_watermark(self) -> float:
        """Fraction of total physical blocks currently pinned by active sequences."""
        return self.active_block_count / self.num_blocks

    @property
    def prefix_hit_rate(self) -> float:
        total = self.prefix_cache_hits + self.prefix_cache_misses
        return 0.0 if total == 0 else self.prefix_cache_hits / total

    def get_block(self, block_id: int) -> PhysicalBlock:
        return self._blocks[block_id]

    def get_block_table(self, seq_id: str) -> BlockTable:
        return self._tables[seq_id]

    def estimate_prefix_hit_rate(self, prompt_tokens: Sequence[int]) -> float:
        """Estimate APC block hit rate for ``prompt_tokens`` without mutating state."""
        full_blocks = len(prompt_tokens) // self.block_size
        if full_blocks == 0:
            return 0.0
        hits = 0
        parent_hash: str | None = None
        for idx in range(full_blocks):
            chunk = prompt_tokens[idx * self.block_size : (idx + 1) * self.block_size]
            block_hash = compute_block_hash(parent_hash, chunk)
            if block_hash in self._prefix_index:
                hits += 1
                parent_hash = block_hash
            else:
                break
        return hits / full_blocks

    def _acquire_fresh_block(self) -> PhysicalBlock:
        if self._free_ids:
            block_id = self._free_ids.pop()
            block = self._blocks[block_id]
        elif self._evictable_lru:
            block_id, old_hash = self._evictable_lru.popitem(last=False)
            self._prefix_index.pop(old_hash, None)
            block = self._blocks[block_id]
            self.evictions += 1
        else:
            raise OutOfKVBlocksError(
                f"Exhausted all {self.num_blocks} physical KV blocks"
            )
        block.refcount = 1
        block.prefix_hash = None
        block.tokens.clear()
        return block

    def _acquire_full_block(
        self, parent_hash: str | None, chunk: Sequence[int]
    ) -> tuple[PhysicalBlock, str]:
        block_hash = compute_block_hash(parent_hash, chunk)
        cached_id = self._prefix_index.get(block_hash)
        if cached_id is not None:
            block = self._blocks[cached_id]
            if block.refcount == 0:
                self._evictable_lru.pop(cached_id, None)
            block.refcount += 1
            self.prefix_cache_hits += 1
            return block, block_hash

        self.prefix_cache_misses += 1
        block = self._acquire_fresh_block()
        block.tokens = [int(t) for t in chunk]
        block.prefix_hash = block_hash
        self._prefix_index[block_hash] = block.block_id
        return block, block_hash

    def _release_block(self, block_id: int) -> None:
        block = self._blocks[block_id]
        if block.refcount <= 0:
            raise RuntimeError(f"Double-free detected on block {block_id}")
        block.refcount -= 1
        if block.refcount == 0:
            if block.prefix_hash is not None and block.is_full:
                self._evictable_lru[block_id] = block.prefix_hash
                self._evictable_lru.move_to_end(block_id)
            else:
                block.tokens.clear()
                block.prefix_hash = None
                self._free_ids.append(block_id)

    def allocate_sequence(self, seq_id: str, prompt_tokens: Sequence[int]) -> BlockTable:
        """Allocate physical KV blocks for a sequence, reusing APC prefix blocks."""
        if seq_id in self._tables:
            raise ValueError(f"Sequence {seq_id!r} already exists")

        tokens = [int(t) for t in prompt_tokens]
        allocated_ids: list[int] = []
        parent_hash: str | None = None
        try:
            full_count = len(tokens) // self.block_size
            for idx in range(full_count):
                chunk = tokens[idx * self.block_size : (idx + 1) * self.block_size]
                block, parent_hash = self._acquire_full_block(parent_hash, chunk)
                allocated_ids.append(block.block_id)

            rem_start = full_count * self.block_size
            if rem_start < len(tokens):
                tail_block = self._acquire_fresh_block()
                tail_block.tokens = tokens[rem_start:]
                allocated_ids.append(tail_block.block_id)
        except OutOfKVBlocksError:
            for bid in allocated_ids:
                self._release_block(bid)
            raise

        table = BlockTable(
            seq_id=seq_id,
            block_size=self.block_size,
            physical_block_ids=allocated_ids,
            token_ids=tokens,
        )
        self._tables[seq_id] = table
        return table

    def fork_sequence(self, parent_seq_id: str, child_seq_id: str) -> BlockTable:
        """Copy-on-Write fork: O(num_blocks) metadata clone with refcount increment."""
        if parent_seq_id not in self._tables:
            raise KeyError(f"Unknown parent sequence {parent_seq_id!r}")
        if child_seq_id in self._tables:
            raise ValueError(f"Child sequence {child_seq_id!r} already exists")

        parent = self._tables[parent_seq_id]
        for bid in parent.physical_block_ids:
            self._blocks[bid].refcount += 1

        child = BlockTable(
            seq_id=child_seq_id,
            block_size=self.block_size,
            physical_block_ids=list(parent.physical_block_ids),
            token_ids=list(parent.token_ids),
        )
        self._tables[child_seq_id] = child
        return child

    def _parent_hash_for_tail(self, table: BlockTable) -> str | None:
        if len(table.physical_block_ids) < 2:
            return None
        return self._blocks[table.physical_block_ids[-2]].prefix_hash

    def append_token(self, seq_id: str, token_id: int) -> int:
        """Append one decoded token to ``seq_id``, applying Copy-on-Write if shared."""
        if seq_id not in self._tables:
            raise KeyError(f"Unknown sequence {seq_id!r}")

        table = self._tables[seq_id]
        tok = int(token_id)

        if not table.physical_block_ids or self._blocks[table.physical_block_ids[-1]].is_full:
            new_block = self._acquire_fresh_block()
            new_block.tokens.append(tok)
            table.physical_block_ids.append(new_block.block_id)
            table.token_ids.append(tok)
            return new_block.block_id

        tail_id = table.physical_block_ids[-1]
        tail_block = self._blocks[tail_id]

        if tail_block.refcount > 1:
            new_block = self._acquire_fresh_block()
            new_block.tokens = list(tail_block.tokens)
            self._release_block(tail_id)
            table.physical_block_ids[-1] = new_block.block_id
            tail_block = new_block
            self.cow_copies += 1

        tail_block.tokens.append(tok)
        table.token_ids.append(tok)

        if tail_block.is_full:
            parent_hash = self._parent_hash_for_tail(table)
            block_hash = compute_block_hash(parent_hash, tail_block.tokens)
            existing_id = self._prefix_index.get(block_hash)
            if existing_id is not None and existing_id != tail_block.block_id:
                existing = self._blocks[existing_id]
                if existing.refcount == 0:
                    self._evictable_lru.pop(existing_id, None)
                existing.refcount += 1
                self.prefix_cache_hits += 1
                old_id = tail_block.block_id
                table.physical_block_ids[-1] = existing_id
                self._release_block(old_id)
                return existing_id
            tail_block.prefix_hash = block_hash
            self._prefix_index[block_hash] = tail_block.block_id

        return tail_block.block_id

    def free_sequence(self, seq_id: str) -> int:
        """Release all physical blocks held by ``seq_id``; returns blocks decremented."""
        table = self._tables.pop(seq_id)
        for bid in table.physical_block_ids:
            self._release_block(bid)
        return len(table.physical_block_ids)


class WorkerRole(Enum):
    PREFILL = "prefill"
    DECODE = "decode"


@dataclass(frozen=True, slots=True)
class PreemptionEvent:
    """Telemetry record emitted when KV memory pressure triggers sequence preemption."""

    seq_id: str
    worker_id: str
    role: WorkerRole
    freed_blocks: int
    watermark_before: float
    watermark_after: float
    reason: str


@dataclass(frozen=True, slots=True)
class RoutingDecision:
    """Placement decision emitted by ``LlmdDisaggregatedScheduler``."""

    seq_id: str
    prefill_worker_id: str
    decode_worker_id: str
    estimated_prefix_hit_rate: float
    prefill_watermark: float
    decode_watermark: float
    preempted_sequences: tuple[str, ...] = ()


@dataclass(slots=True)
class DisaggregatedWorker:
    """Single Prefill or Decode worker in an ``llm-d`` serving pool."""

    worker_id: str
    role: WorkerRole
    allocator: PhysicalBlockAllocator
    kv_high_watermark: float = 0.85
    active_priorities: dict[str, int] = field(default_factory=dict)

    @property
    def kv_watermark(self) -> float:
        return self.allocator.kv_watermark


class LlmdDisaggregatedScheduler:
    """``llm-d`` disaggregated Prefill/Decode router with KV watermark preemption."""

    def __init__(
        self,
        prefill_workers: Sequence[DisaggregatedWorker],
        decode_workers: Sequence[DisaggregatedWorker],
    ) -> None:
        if not prefill_workers:
            raise ValueError("At least one prefill worker is required")
        if not decode_workers:
            raise ValueError("At least one decode worker is required")
        self.prefill_workers: dict[str, DisaggregatedWorker] = {
            w.worker_id: w for w in prefill_workers
        }
        self.decode_workers: dict[str, DisaggregatedWorker] = {
            w.worker_id: w for w in decode_workers
        }
        self.preemption_events: list[PreemptionEvent] = []
        self.preempted_queue: list[tuple[str, list[int], int]] = []
        self.routing_history: list[RoutingDecision] = []

    def _select_prefill_worker(
        self, prompt_tokens: Sequence[int]
    ) -> tuple[DisaggregatedWorker, float]:
        best_worker: DisaggregatedWorker | None = None
        best_score = -1e9
        best_hit_rate = 0.0
        for worker in self.prefill_workers.values():
            hit_rate = worker.allocator.estimate_prefix_hit_rate(prompt_tokens)
            headroom = 1.0 - worker.kv_watermark
            watermark_penalty = 0.5 if worker.kv_watermark >= worker.kv_high_watermark else 0.0
            score = (2.0 * hit_rate) + headroom - watermark_penalty
            if score > best_score:
                best_score = score
                best_worker = worker
                best_hit_rate = hit_rate
        assert best_worker is not None
        return best_worker, best_hit_rate

    def _select_decode_worker(self) -> DisaggregatedWorker:
        return min(self.decode_workers.values(), key=lambda w: w.kv_watermark)

    def _ensure_headroom(
        self,
        worker: DisaggregatedWorker,
        needed_blocks: int,
        incoming_priority: int,
        reason: str,
    ) -> list[str]:
        preempted: list[str] = []
        while worker.allocator.available_block_count < needed_blocks or (
            worker.kv_watermark >= worker.kv_high_watermark and worker.active_priorities
        ):
            if not worker.active_priorities:
                break
            victim_id = min(
                worker.active_priorities,
                key=lambda sid: worker.active_priorities[sid],
            )
            victim_prio = worker.active_priorities[victim_id]
            if (
                victim_prio > incoming_priority
                and worker.allocator.available_block_count >= needed_blocks
            ):
                break
            watermark_before = worker.kv_watermark
            victim_tokens = list(worker.allocator.get_block_table(victim_id).token_ids)
            freed = worker.allocator.free_sequence(victim_id)
            worker.active_priorities.pop(victim_id, None)
            watermark_after = worker.kv_watermark
            self.preemption_events.append(
                PreemptionEvent(
                    seq_id=victim_id,
                    worker_id=worker.worker_id,
                    role=worker.role,
                    freed_blocks=freed,
                    watermark_before=watermark_before,
                    watermark_after=watermark_after,
                    reason=reason,
                )
            )
            self.preempted_queue.append((victim_id, victim_tokens, victim_prio))
            preempted.append(victim_id)
        return preempted

    def schedule_request(
        self,
        seq_id: str,
        prompt_tokens: Sequence[int],
        *,
        priority: int = 0,
    ) -> RoutingDecision:
        """Route a request through Prefill (APC-aware) and transfer KV to Decode."""
        prefill_worker, hit_rate = self._select_prefill_worker(prompt_tokens)
        decode_worker = self._select_decode_worker()

        blocks_needed = max(
            1,
            (len(prompt_tokens) + prefill_worker.allocator.block_size - 1)
            // prefill_worker.allocator.block_size,
        )

        preempted: list[str] = []
        preempted.extend(
            self._ensure_headroom(
                prefill_worker,
                needed_blocks=blocks_needed,
                incoming_priority=priority,
                reason="prefill_kv_watermark_exceeded",
            )
        )
        prefill_worker.allocator.allocate_sequence(seq_id, prompt_tokens)
        prefill_worker.active_priorities[seq_id] = priority
        prefill_wm = prefill_worker.kv_watermark

        prefill_worker.allocator.free_sequence(seq_id)
        prefill_worker.active_priorities.pop(seq_id, None)

        preempted.extend(
            self._ensure_headroom(
                decode_worker,
                needed_blocks=blocks_needed,
                incoming_priority=priority,
                reason="decode_kv_watermark_exceeded",
            )
        )
        decode_worker.allocator.allocate_sequence(seq_id, prompt_tokens)
        decode_worker.active_priorities[seq_id] = priority
        decode_wm = decode_worker.kv_watermark

        decision = RoutingDecision(
            seq_id=seq_id,
            prefill_worker_id=prefill_worker.worker_id,
            decode_worker_id=decode_worker.worker_id,
            estimated_prefix_hit_rate=hit_rate,
            prefill_watermark=prefill_wm,
            decode_watermark=decode_wm,
            preempted_sequences=tuple(preempted),
        )
        self.routing_history.append(decision)
        return decision

    def telemetry_snapshot(self) -> dict[str, object]:
        return {
            "prefill_workers": {
                wid: {
                    "kv_watermark": w.kv_watermark,
                    "prefix_hit_rate": w.allocator.prefix_hit_rate,
                    "evictable_cached_blocks": w.allocator.evictable_cached_count,
                }
                for wid, w in self.prefill_workers.items()
            },
            "decode_workers": {
                wid: {
                    "kv_watermark": w.kv_watermark,
                    "active_sequences": len(w.active_priorities),
                    "cow_copies": w.allocator.cow_copies,
                }
                for wid, w in self.decode_workers.items()
            },
            "total_preemptions": len(self.preemption_events),
            "preempted_queue_depth": len(self.preempted_queue),
            "total_routed": len(self.routing_history),
        }
