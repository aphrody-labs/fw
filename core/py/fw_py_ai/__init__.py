"""Fairwind Python AI Kernel Bridge (`fw_py_ai`).

Zero-dependency Python 3.12+ stdlib package unlocking PyTorch/Triton/vLLM/llm-d
capabilities alongside Fairwind's Rust workspace (`fw-ggml`, `fw-compute`):
- ``ggml_dlpack``: Zero-copy ``memoryview``/DLPack tensor views, GGUF v3 codec, Q8_0 parity
- ``vllm_paged_kv``: PagedAttention allocator, chained SHA-256 APC, CoW fork, llm-d scheduler
- ``pyo3_uv_bridge``: Pinned ``uv``/``ruff``/``pyo3`` metadata, PEP 723 scripts, PyO3 abi3 FFI
- ``migrate_hermes_to_fw``: Zero-dep ``~/.hermes/`` -> ``fw`` config/skills/memory/cron migrator
"""

from __future__ import annotations

from .ggml_dlpack import (
    GGUF_DEFAULT_ALIGNMENT,
    GGUF_HEADER_BYTES,
    GGUF_MAGIC,
    Q8_0_BLOCK_SIZE,
    DLDataTypeCode,
    DLDeviceType,
    DLPackDescriptor,
    FwTensorView,
    GgmlType,
    GgufHeader,
    Q8_0Block,
    dequantize_q8_0,
    dot_q8_0,
    encode_gguf_header,
    parse_gguf_header,
    quantize_q8_0,
    quantize_q8_0_block,
)
from .migrate_hermes_to_fw import (
    MigrationAction,
    MigrationReport,
    migrate_hermes_to_fw,
)
from .pyo3_uv_bridge import (
    PINNED_FORKS,
    ForkPin,
    Pep723ScriptSpec,
    PyO3Abi3ContractSpec,
    generate_gpu_eval_script,
    generate_pep723_script,
    generate_pyo3_abi3_rust_bridge,
    generate_pyo3_cargo_toml,
    load_fork_metadata,
    parse_pep723_metadata,
)
from .vllm_paged_kv import (
    DEFAULT_BLOCK_SIZE,
    BlockTable,
    DisaggregatedWorker,
    LlmdDisaggregatedScheduler,
    OutOfKVBlocksError,
    PhysicalBlock,
    PhysicalBlockAllocator,
    PreemptionEvent,
    RoutingDecision,
    WorkerRole,
    compute_block_hash,
)

__all__ = [
    "DEFAULT_BLOCK_SIZE",
    "GGUF_DEFAULT_ALIGNMENT",
    "GGUF_HEADER_BYTES",
    "GGUF_MAGIC",
    "PINNED_FORKS",
    "Q8_0_BLOCK_SIZE",
    "BlockTable",
    "DLDataTypeCode",
    "DLDeviceType",
    "DLPackDescriptor",
    "DisaggregatedWorker",
    "ForkPin",
    "FwTensorView",
    "GgmlType",
    "GgufHeader",
    "LlmdDisaggregatedScheduler",
    "MigrationAction",
    "MigrationReport",
    "OutOfKVBlocksError",
    "Pep723ScriptSpec",
    "PhysicalBlock",
    "PhysicalBlockAllocator",
    "PreemptionEvent",
    "PyO3Abi3ContractSpec",
    "Q8_0Block",
    "RoutingDecision",
    "WorkerRole",
    "compute_block_hash",
    "dequantize_q8_0",
    "dot_q8_0",
    "encode_gguf_header",
    "generate_gpu_eval_script",
    "generate_pep723_script",
    "generate_pyo3_abi3_rust_bridge",
    "generate_pyo3_cargo_toml",
    "load_fork_metadata",
    "migrate_hermes_to_fw",
    "parse_gguf_header",
    "parse_pep723_metadata",
    "quantize_q8_0",
    "quantize_q8_0_block",
]
