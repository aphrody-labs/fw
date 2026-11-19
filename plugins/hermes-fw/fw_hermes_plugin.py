"""Native Python/uv Fairwind (``fw``) plugin for Hermes Agent.

Follows Hermes Agent's "Plugins never touch core" contract: exposes ``fw bench``,
``fw grep``/``find``/``ls``, ``ggml_dlpack``, and ``vllm_paged_kv`` through
``register(ctx)`` with zero external pip dependencies.
"""

from __future__ import annotations

import fnmatch
import re
import shutil
import subprocess
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

_REPO_ROOT = Path(__file__).resolve().parents[2]
_CORE_PY = _REPO_ROOT / "core" / "py"
if _CORE_PY.is_dir() and str(_CORE_PY) not in sys.path:
    sys.path.insert(0, str(_CORE_PY))

from fw_py_ai.ggml_dlpack import (
    DLDeviceType,
    FwTensorView,
    dot_q8_0,
    encode_gguf_header,
    parse_gguf_header,
    quantize_q8_0,
)
from fw_py_ai.vllm_paged_kv import (
    DEFAULT_BLOCK_SIZE,
    DisaggregatedWorker,
    LlmdDisaggregatedScheduler,
    PhysicalBlockAllocator,
    WorkerRole,
)

PLUGIN_ID = "hermes-fw"
PLUGIN_VERSION = "0.1.0"


@dataclass(frozen=True, slots=True)
class HermesToolSpec:
    """Declarative Hermes tool descriptor."""

    name: str
    description: str
    parameters: dict[str, Any]
    handler: Callable[..., dict[str, Any]]


def _find_fw_binary() -> str | None:
    local_release = _REPO_ROOT / "target" / "release" / ("fw.exe" if sys.platform == "win32" else "fw")
    if local_release.is_file():
        return str(local_release)
    return shutil.which("fw")


def _run_subprocess(cmd: list[str], cwd: str | None = None, timeout: float = 15.0) -> dict[str, Any]:
    proc = subprocess.run(
        cmd,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    return {
        "ok": proc.returncode == 0,
        "returncode": proc.returncode,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "cmd": cmd,
    }


def tool_fw_bench(*, iterations: int = 32, use_cli: bool = False) -> dict[str, Any]:
    """Run ``fw bench`` CLI when requested and available, or execute in-process Q8_0 kernel bench."""
    if use_cli:
        fw_bin = _find_fw_binary()
        if fw_bin is not None:
            return _run_subprocess([fw_bin, "bench"])

    vec_a = [float((i % 17) - 8) * 0.25 for i in range(128)]
    vec_b = [float((i % 13) - 6) * 0.30 for i in range(128)]
    t0 = time.perf_counter()
    last_dot = 0.0
    for _ in range(max(1, iterations)):
        qa = quantize_q8_0(vec_a)
        qb = quantize_q8_0(vec_b)
        last_dot = dot_q8_0(qa, qb)
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    return {
        "ok": True,
        "engine": "fw_py_ai.ggml_dlpack.q8_0",
        "iterations": max(1, iterations),
        "elapsed_ms": round(elapsed_ms, 4),
        "last_dot": last_dot,
    }


def tool_fw_grep(
    *,
    pattern: str,
    root: str = ".",
    max_matches: int = 50,
) -> dict[str, Any]:
    """Search files under ``root`` matching regex ``pattern`` (stdlib fallback, zero shell)."""
    root_path = Path(root).resolve()
    regex = re.compile(pattern)
    matches: list[dict[str, Any]] = []
    if root_path.is_file():
        candidates = [root_path]
    else:
        candidates = sorted(p for p in root_path.rglob("*") if p.is_file())
    for path in candidates:
        if len(matches) >= max_matches:
            break
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for line_no, line in enumerate(text.splitlines(), start=1):
            if regex.search(line):
                matches.append(
                    {
                        "path": str(path.relative_to(root_path)) if root_path.is_dir() else path.name,
                        "line": line_no,
                        "text": line.strip(),
                    }
                )
                if len(matches) >= max_matches:
                    break
    return {"ok": True, "root": str(root_path), "pattern": pattern, "matches": matches}


def tool_fw_find(
    *,
    glob_pattern: str = "*",
    root: str = ".",
    max_results: int = 100,
) -> dict[str, Any]:
    """Find files matching ``glob_pattern`` under ``root``."""
    root_path = Path(root).resolve()
    found: list[str] = []
    if root_path.is_dir():
        for path in sorted(root_path.rglob("*")):
            rel = path.relative_to(root_path).as_posix()
            if fnmatch.fnmatch(path.name, glob_pattern) or fnmatch.fnmatch(rel, glob_pattern):
                found.append(rel)
                if len(found) >= max_results:
                    break
    return {"ok": True, "root": str(root_path), "glob": glob_pattern, "files": found}


def tool_fw_ls(*, path: str = ".") -> dict[str, Any]:
    """List directory entries with type and byte size."""
    target = Path(path).resolve()
    if not target.exists():
        return {"ok": False, "error": f"Path not found: {target}"}
    entries: list[dict[str, Any]] = []
    if target.is_dir():
        for child in sorted(target.iterdir(), key=lambda p: p.name):
            entries.append(
                {
                    "name": child.name,
                    "kind": "dir" if child.is_dir() else "file",
                    "size": child.stat().st_size if child.is_file() else 0,
                }
            )
    else:
        entries.append({"name": target.name, "kind": "file", "size": target.stat().st_size})
    return {"ok": True, "path": str(target), "entries": entries}


def tool_fw_ggml_dlpack(
    *,
    action: str = "q8_0_dot",
    a: list[float] | None = None,
    b: list[float] | None = None,
    tensor_count: int = 16,
    kv_count: int = 64,
) -> dict[str, Any]:
    """Expose ``ggml_dlpack`` zero-copy DLPack views, GGUF v3 codec, and Q8_0 dot kernels."""
    if action == "gguf_header":
        raw = encode_gguf_header(3, tensor_count, kv_count)
        parsed = parse_gguf_header(raw)
        return {
            "ok": True,
            "action": action,
            "header_hex": raw.hex(),
            "byte_len": len(raw),
            "version": parsed.version,
            "tensor_count": parsed.tensor_count,
            "kv_count": parsed.kv_count,
        }
    if action == "dlpack_view":
        vals = a or [1.0, 2.0, 3.0, 4.0]
        view = FwTensorView.from_f32_sequence(vals, device_type=DLDeviceType.CPU)
        return {
            "ok": True,
            "action": action,
            "descriptor": view.descriptor.to_dict(),
            "dlpack_device": view.__dlpack_device__(),
        }
    vec_a = a or [1.0, 2.0, 3.0, 4.0]
    vec_b = b or [4.0, 3.0, 2.0, 1.0]
    qa = quantize_q8_0(vec_a)
    qb = quantize_q8_0(vec_b)
    return {
        "ok": True,
        "action": "q8_0_dot",
        "blocks": len(qa),
        "dot": dot_q8_0(qa, qb),
    }


def tool_fw_vllm_paged_kv(
    *,
    num_blocks: int = 16,
    prefix_tokens: list[int] | None = None,
    branch_token: int = 999,
) -> dict[str, Any]:
    """Simulate vLLM PagedAttention APC, CoW fork, and llm-d Prefill/Decode routing."""
    prefix = prefix_tokens or list(range(DEFAULT_BLOCK_SIZE * 2))
    alloc = PhysicalBlockAllocator(num_blocks=num_blocks, block_size=DEFAULT_BLOCK_SIZE)
    parent = alloc.allocate_sequence("parent", [*prefix, 101, 102])
    child = alloc.fork_sequence("parent", "child")
    alloc.append_token("child", branch_token)

    p0 = DisaggregatedWorker("prefill-0", WorkerRole.PREFILL, PhysicalBlockAllocator(num_blocks))
    d0 = DisaggregatedWorker("decode-0", WorkerRole.DECODE, PhysicalBlockAllocator(num_blocks))
    sched = LlmdDisaggregatedScheduler([p0], [d0])
    sched.schedule_request("r1", [*prefix, 1])
    dec2 = sched.schedule_request("r2", [*prefix, 2])

    return {
        "ok": True,
        "parent_blocks": parent.physical_block_ids,
        "child_blocks": child.physical_block_ids,
        "cow_copies": alloc.cow_copies,
        "apc_hit_rate_r2": dec2.estimated_prefix_hit_rate,
        "scheduler_telemetry": sched.telemetry_snapshot(),
    }


def get_tool_specs() -> list[HermesToolSpec]:
    """Return all Hermes tools provided by the ``hermes-fw`` plugin."""
    return [
        HermesToolSpec(
            name="fw_bench",
            description="Run Fairwind CLI or in-process Q8_0 microbenchmark.",
            parameters={
                "type": "object",
                "properties": {
                    "iterations": {"type": "integer", "default": 32},
                    "use_cli": {"type": "boolean", "default": False},
                },
            },
            handler=tool_fw_bench,
        ),
        HermesToolSpec(
            name="fw_grep",
            description="Search files with regex under a workspace directory.",
            parameters={
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": {"type": "string"},
                    "root": {"type": "string", "default": "."},
                    "max_matches": {"type": "integer", "default": 50},
                },
            },
            handler=tool_fw_grep,
        ),
        HermesToolSpec(
            name="fw_find",
            description="Find files matching a glob pattern under a workspace directory.",
            parameters={
                "type": "object",
                "properties": {
                    "glob_pattern": {"type": "string", "default": "*"},
                    "root": {"type": "string", "default": "."},
                    "max_results": {"type": "integer", "default": 100},
                },
            },
            handler=tool_fw_find,
        ),
        HermesToolSpec(
            name="fw_ls",
            description="List directory entries with type and byte sizes.",
            parameters={
                "type": "object",
                "properties": {"path": {"type": "string", "default": "."}},
            },
            handler=tool_fw_ls,
        ),
        HermesToolSpec(
            name="fw_ggml_dlpack",
            description="Run ggml_dlpack GGUF v3 header codec, DLPack view, or Q8_0 dot product.",
            parameters={
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["q8_0_dot", "gguf_header", "dlpack_view"],
                        "default": "q8_0_dot",
                    },
                },
            },
            handler=tool_fw_ggml_dlpack,
        ),
        HermesToolSpec(
            name="fw_vllm_paged_kv",
            description="Run vLLM PagedAttention APC/CoW and llm-d disaggregated KV scheduling.",
            parameters={
                "type": "object",
                "properties": {
                    "num_blocks": {"type": "integer", "default": 16},
                    "branch_token": {"type": "integer", "default": 999},
                },
            },
            handler=tool_fw_vllm_paged_kv,
        ),
    ]


def register(ctx: Any) -> list[str]:
    """Hermes Agent plugin entrypoint: register all Fairwind tools on ``ctx``."""
    registered: list[str] = []
    for spec in get_tool_specs():
        if hasattr(ctx, "register_tool"):
            ctx.register_tool(
                name=spec.name,
                description=spec.description,
                parameters=spec.parameters,
                handler=spec.handler,
            )
        elif isinstance(ctx, dict):
            ctx[spec.name] = spec
        else:
            raise TypeError(f"Unsupported Hermes plugin context: {type(ctx)!r}")
        registered.append(spec.name)
    return registered
