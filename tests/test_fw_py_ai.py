"""Unit tests for core/py/fw_py_ai and plugins/hermes-fw."""

from __future__ import annotations

import array
import importlib.util
import json
import math
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any

import tomllib

REPO_ROOT = Path(__file__).resolve().parents[1]
CORE_PY = REPO_ROOT / "core" / "py"
if str(CORE_PY) not in sys.path:
    sys.path.insert(0, str(CORE_PY))

from fw_py_ai import (
    DEFAULT_BLOCK_SIZE,
    GGUF_MAGIC,
    PINNED_FORKS,
    Q8_0_BLOCK_SIZE,
    DisaggregatedWorker,
    DLDataTypeCode,
    DLDeviceType,
    FwTensorView,
    GgmlType,
    LlmdDisaggregatedScheduler,
    OutOfKVBlocksError,
    Pep723ScriptSpec,
    PhysicalBlockAllocator,
    PyO3Abi3ContractSpec,
    Q8_0Block,
    WorkerRole,
    compute_block_hash,
    dequantize_q8_0,
    dot_q8_0,
    encode_gguf_header,
    generate_gpu_eval_script,
    generate_pep723_script,
    generate_pyo3_abi3_rust_bridge,
    generate_pyo3_cargo_toml,
    load_fork_metadata,
    migrate_hermes_to_fw,
    parse_gguf_header,
    parse_pep723_metadata,
    quantize_q8_0,
)


def _load_hermes_fw_plugin() -> Any:
    plugin_path = REPO_ROOT / "plugins" / "hermes-fw" / "fw_hermes_plugin.py"
    spec = importlib.util.spec_from_file_location("fw_hermes_plugin", plugin_path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class TestGgmlDlpack(unittest.TestCase):
    def test_zero_copy_memoryview_and_dlpack_descriptor(self) -> None:
        backing = array.array("f", [1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        tensor = FwTensorView(
            backing,
            dtype="float32",
            shape=(3, 2),
            device_type=DLDeviceType.CPU,
        )
        desc = tensor.descriptor
        self.assertEqual(desc.dtype, "float32")
        self.assertEqual(desc.shape, (3, 2))
        self.assertEqual(desc.strides, (2, 1))
        self.assertEqual(desc.device_type, DLDeviceType.CPU)
        self.assertEqual(desc.byte_offset, 0)
        self.assertEqual(desc.data_ptr_len, 24)
        self.assertEqual(desc.dlpack_dtype_tuple(), (int(DLDataTypeCode.FLOAT), 32, 1))
        self.assertTrue(desc.is_contiguous())
        self.assertEqual(tensor.__dlpack_device__(), (int(DLDeviceType.CPU), 0))
        self.assertIsNotNone(tensor.data_ptr_address())

        sub = tensor.slice_dim0(1, 3)
        self.assertEqual(sub.shape, (2, 2))
        self.assertEqual(sub.byte_offset, 8)
        self.assertEqual(sub.data_ptr_len, 16)
        self.assertEqual(sub.as_f32_list(), [3.0, 4.0, 5.0, 6.0])

        sub.write_f32_at(0, 99.5)
        self.assertAlmostEqual(backing[2], 99.5, places=5)
        self.assertAlmostEqual(tensor.as_f32_list()[2], 99.5, places=5)

    def test_tensor_view_bounds_check(self) -> None:
        with self.assertRaises(ValueError):
            FwTensorView(bytearray(8), dtype="float32", shape=(3,))

    def test_gguf_v3_header_exact_binary_layout(self) -> None:
        raw = encode_gguf_header(3, 42, 128)
        self.assertEqual(len(raw), 24)
        self.assertEqual(raw[:4], GGUF_MAGIC)
        self.assertEqual(raw, struct.pack("<4sIQQ", b"GGUF", 3, 42, 128))

        parsed = parse_gguf_header(raw)
        self.assertEqual(parsed.version, 3)
        self.assertEqual(parsed.tensor_count, 42)
        self.assertEqual(parsed.metadata_kv_count, 128)
        self.assertEqual(parsed.kv_count, 128)

        with self.assertRaises(ValueError):
            parse_gguf_header(b"GGUF\x03\x00")
        with self.assertRaises(ValueError):
            parse_gguf_header(b"NOPE" + raw[4:])
        with self.assertRaises(ValueError):
            parse_gguf_header(encode_gguf_header(99, 1, 1))

    def test_ggml_type_parity_with_fw_ggml(self) -> None:
        self.assertEqual(GgmlType.F32.block_size, 1)
        self.assertEqual(GgmlType.F32.type_size, 4)
        self.assertEqual(GgmlType.Q8_0.block_size, 32)
        self.assertEqual(GgmlType.Q8_0.type_size, 34)
        self.assertEqual(GgmlType.Q4K.block_size, 256)
        self.assertEqual(GgmlType.Q4K.type_size, 144)

    def test_q8_0_quantize_dequantize_dot_and_wire_roundtrip(self) -> None:
        a = [1.0, 2.0, 3.0, 4.0]
        b = [4.0, 3.0, 2.0, 1.0]
        qa = quantize_q8_0(a)
        qb = quantize_q8_0(b)
        self.assertEqual(len(qa), 1)
        d = dot_q8_0(qa, qb)
        self.assertLess(abs(d - 20.0), 0.3)

        v1 = [math.sin(i * 0.13) * 3.5 for i in range(Q8_0_BLOCK_SIZE * 2)]
        v2 = [math.cos(i * 0.17) * 2.5 for i in range(Q8_0_BLOCK_SIZE * 2)]
        exact_dot = sum(x * y for x, y in zip(v1, v2, strict=True))
        b1 = quantize_q8_0(v1)
        b2 = quantize_q8_0(v2)
        self.assertEqual(len(b1), 2)
        self.assertEqual(len(b1[0].quants), Q8_0_BLOCK_SIZE)
        q_dot = dot_q8_0(b1, b2)
        self.assertLess(abs(q_dot - exact_dot), 1.0)

        deq = dequantize_q8_0(b1)
        self.assertEqual(len(deq), len(v1))
        max_err = max(abs(x - y) for x, y in zip(v1, deq, strict=True))
        self.assertLess(max_err, 0.05)

        packed = b1[0].to_bytes()
        self.assertEqual(len(packed), 36)
        unpacked = Q8_0Block.from_bytes(packed, original_len=Q8_0_BLOCK_SIZE)
        self.assertEqual(unpacked, b1[0])

        zeros = quantize_q8_0([0.0] * Q8_0_BLOCK_SIZE)
        self.assertEqual(zeros[0].scale, 1.0)
        self.assertEqual(zeros[0].quants, (0,) * Q8_0_BLOCK_SIZE)


class TestVllmPagedKv(unittest.TestCase):
    def test_apc_chained_sha256_prefix_hits_and_eviction(self) -> None:
        alloc = PhysicalBlockAllocator(num_blocks=6, block_size=DEFAULT_BLOCK_SIZE)
        prefix = list(range(100, 100 + 2 * DEFAULT_BLOCK_SIZE))
        seq_a_tokens = prefix + [900, 901, 902]
        seq_b_tokens = prefix + [950, 951, 952]

        h0 = compute_block_hash(None, prefix[:16])
        h1 = compute_block_hash(h0, prefix[16:32])
        self.assertNotEqual(h0, h1)
        self.assertEqual(len(h1), 64)

        ta = alloc.allocate_sequence("seq-a", seq_a_tokens)
        self.assertEqual(ta.num_blocks, 3)
        self.assertEqual(alloc.prefix_cache_misses, 2)
        self.assertEqual(alloc.prefix_cache_hits, 0)

        tb = alloc.allocate_sequence("seq-b", seq_b_tokens)
        self.assertEqual(tb.physical_block_ids[:2], ta.physical_block_ids[:2])
        self.assertNotEqual(tb.physical_block_ids[2], ta.physical_block_ids[2])
        self.assertEqual(alloc.prefix_cache_hits, 2)
        self.assertEqual(alloc.get_block(ta.physical_block_ids[0]).refcount, 2)
        self.assertEqual(alloc.get_block(ta.physical_block_ids[1]).refcount, 2)

        alloc.free_sequence("seq-a")
        alloc.free_sequence("seq-b")
        self.assertEqual(alloc.active_block_count, 0)
        self.assertEqual(alloc.evictable_cached_count, 2)

        tc = alloc.allocate_sequence("seq-c", prefix)
        self.assertEqual(tc.physical_block_ids, ta.physical_block_ids[:2])
        self.assertEqual(alloc.prefix_cache_hits, 4)
        self.assertEqual(alloc.evictable_cached_count, 0)

    def test_cow_fork_sequence_refcount_and_isolation(self) -> None:
        alloc = PhysicalBlockAllocator(num_blocks=8, block_size=DEFAULT_BLOCK_SIZE)
        parent = alloc.allocate_sequence("beam-0", list(range(20)))
        child = alloc.fork_sequence("beam-0", "beam-1")

        self.assertEqual(parent.physical_block_ids, child.physical_block_ids)
        for bid in parent.physical_block_ids:
            self.assertEqual(alloc.get_block(bid).refcount, 2)

        old_tail_id = child.physical_block_ids[-1]
        new_tail_id = alloc.append_token("beam-1", 777)
        self.assertNotEqual(new_tail_id, old_tail_id)
        self.assertEqual(alloc.cow_copies, 1)
        self.assertEqual(alloc.get_block(old_tail_id).refcount, 1)
        self.assertEqual(alloc.get_block(new_tail_id).refcount, 1)
        self.assertEqual(parent.token_ids[-1], 19)
        self.assertEqual(child.token_ids[-1], 777)
        self.assertEqual(alloc.get_block(parent.physical_block_ids[0]).refcount, 2)

    def test_out_of_kv_blocks_rolls_back_atomically(self) -> None:
        alloc = PhysicalBlockAllocator(num_blocks=2, block_size=DEFAULT_BLOCK_SIZE)
        with self.assertRaises(OutOfKVBlocksError):
            alloc.allocate_sequence("too-big", list(range(3 * DEFAULT_BLOCK_SIZE)))
        self.assertEqual(alloc.active_block_count, 0)

    def test_llmd_disaggregated_scheduler_routing_and_preemption(self) -> None:
        p0 = DisaggregatedWorker("prefill-0", WorkerRole.PREFILL, PhysicalBlockAllocator(8))
        p1 = DisaggregatedWorker("prefill-1", WorkerRole.PREFILL, PhysicalBlockAllocator(8))
        d0 = DisaggregatedWorker(
            "decode-0",
            WorkerRole.DECODE,
            PhysicalBlockAllocator(4),
            kv_high_watermark=0.75,
        )
        d1 = DisaggregatedWorker(
            "decode-1",
            WorkerRole.DECODE,
            PhysicalBlockAllocator(4),
            kv_high_watermark=0.75,
        )
        sched = LlmdDisaggregatedScheduler([p0, p1], [d0, d1])

        shared_prefix = list(range(32))
        dec1 = sched.schedule_request("req-1", shared_prefix + [1, 2], priority=1)
        self.assertEqual(dec1.prefill_worker_id, "prefill-0")

        dec2 = sched.schedule_request("req-2", shared_prefix + [3, 4], priority=1)
        self.assertEqual(dec2.prefill_worker_id, "prefill-0")
        self.assertEqual(dec2.estimated_prefix_hit_rate, 1.0)
        self.assertEqual(dec2.decode_worker_id, "decode-1")

        dec3 = sched.schedule_request("req-3", list(range(200, 240)), priority=10)
        self.assertGreaterEqual(len(dec3.preempted_sequences), 1)
        self.assertGreaterEqual(len(sched.preemption_events), 1)
        event = sched.preemption_events[0]
        self.assertEqual(event.role, WorkerRole.DECODE)
        self.assertGreater(event.watermark_before, event.watermark_after)
        self.assertEqual(event.reason, "decode_kv_watermark_exceeded")

        snap = sched.telemetry_snapshot()
        self.assertEqual(snap["total_routed"], 3)
        self.assertGreaterEqual(snap["total_preemptions"], 1)


class TestPyo3UvBridge(unittest.TestCase):
    def test_live_third_party_fork_versions(self) -> None:
        meta = load_fork_metadata(REPO_ROOT)
        self.assertEqual(meta["uv"].version, "0.13.9")
        self.assertEqual(meta["ruff"].version, "0.17.6")
        self.assertEqual(meta["pyo3"].version, "0.30.0")
        self.assertEqual(meta["uv"].edition, "2024")
        self.assertEqual(meta["ruff"].edition, "2024")
        self.assertEqual(meta["pyo3"].edition, "2024")
        self.assertEqual(PINNED_FORKS["uv"].version, meta["uv"].version)
        self.assertEqual(PINNED_FORKS["ruff"].version, meta["ruff"].version)
        self.assertEqual(PINNED_FORKS["pyo3"].version, meta["pyo3"].version)

    def test_pep723_uv_script_generation_and_tomllib_roundtrip(self) -> None:
        script = generate_gpu_eval_script(
            model_id="meta-llama/Llama-3.3-70B-Instruct",
            backend="triton",
            quantization="q8_0",
        )
        self.assertTrue(script.startswith("# /// script\n"))
        parsed = parse_pep723_metadata(script)
        self.assertEqual(parsed["requires-python"], ">=3.12")
        self.assertIn("triton>=3.2", parsed["dependencies"])
        self.assertIn("vllm>=0.8", parsed["dependencies"])
        self.assertEqual(parsed["tool"]["uv"]["exclude-newer"], "2026-11-19T00:00:00Z")

        custom = generate_pep723_script(
            Pep723ScriptSpec(
                requires_python=">=3.12",
                dependencies=("torch>=2.6",),
                index_url="https://download.pytorch.org/whl/cu128",
                body="print('ok')",
            )
        )
        custom_meta = parse_pep723_metadata(custom)
        self.assertEqual(
            custom_meta["tool"]["uv"]["index-url"],
            "https://download.pytorch.org/whl/cu128",
        )

    def test_pyo3_abi3_cargo_and_rust_bridge_contract(self) -> None:
        spec = PyO3Abi3ContractSpec()
        cargo_toml = generate_pyo3_cargo_toml(spec)
        parsed = tomllib.loads(cargo_toml)
        self.assertEqual(parsed["package"]["name"], "fw_py_native")
        self.assertEqual(parsed["lib"]["crate-type"], ["cdylib"])
        self.assertEqual(parsed["dependencies"]["pyo3"]["version"], "0.30.0")
        self.assertIn("abi3-py312", parsed["dependencies"]["pyo3"]["features"])
        self.assertIn("fw-ggml", parsed["dependencies"])
        self.assertIn("fw-compute", parsed["dependencies"])

        rust_src = generate_pyo3_abi3_rust_bridge(spec)
        self.assertIn("PyBuffer<u8>", rust_src)
        self.assertIn("fw_ggml::", rust_src)
        self.assertIn("fw_compute::", rust_src)
        self.assertIn("parse_gguf_header", rust_src)
        self.assertIn("quantize_q8_0", rust_src)
        self.assertIn("dot_q8_0", rust_src)
        self.assertIn("detect_hardware_profile", rust_src)
        self.assertIn("#[pymodule]", rust_src)


class TestHermesFwPluginAndMigration(unittest.TestCase):
    def test_plugin_toml_and_register_tools(self) -> None:
        manifest_path = REPO_ROOT / "plugins" / "hermes-fw" / "plugin.toml"
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(manifest["plugin"]["id"], "hermes-fw")
        self.assertFalse(manifest["compat"]["touches_core"])
        self.assertTrue(manifest["compat"]["uv_isolated"])

        plugin = _load_hermes_fw_plugin()
        registry: dict[str, Any] = {}
        names = plugin.register(registry)
        self.assertEqual(
            set(names),
            {"fw_bench", "fw_grep", "fw_find", "fw_ls", "fw_ggml_dlpack", "fw_vllm_paged_kv"},
        )
        self.assertEqual(set(names), set(manifest["tools"]["exposed"]))

        bench_res = registry["fw_bench"].handler(iterations=4, use_cli=False)
        self.assertTrue(bench_res["ok"])
        self.assertEqual(bench_res["iterations"], 4)

        gguf_res = registry["fw_ggml_dlpack"].handler(action="gguf_header", tensor_count=8, kv_count=16)
        self.assertTrue(gguf_res["ok"])
        self.assertEqual(gguf_res["byte_len"], 24)
        self.assertEqual(gguf_res["version"], 3)

        dot_res = registry["fw_ggml_dlpack"].handler(action="q8_0_dot")
        self.assertTrue(dot_res["ok"])
        self.assertLess(abs(dot_res["dot"] - 20.0), 0.3)

        kv_res = registry["fw_vllm_paged_kv"].handler(num_blocks=16)
        self.assertTrue(kv_res["ok"])
        self.assertEqual(kv_res["cow_copies"], 1)
        self.assertEqual(kv_res["apc_hit_rate_r2"], 1.0)

        with tempfile.TemporaryDirectory() as tmp:
            sample = Path(tmp) / "hello.txt"
            sample.write_text("alpha\nfairwind-kernel\nomega\n", encoding="utf-8")
            grep_res = registry["fw_grep"].handler(pattern="fairwind-kernel", root=tmp)
            self.assertEqual(len(grep_res["matches"]), 1)
            find_res = registry["fw_find"].handler(glob_pattern="*.txt", root=tmp)
            self.assertEqual(find_res["files"], ["hello.txt"])
            ls_res = registry["fw_ls"].handler(path=tmp)
            self.assertEqual(ls_res["entries"][0]["name"], "hello.txt")

    def test_migrate_hermes_to_fw_dry_run_and_apply(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / ".hermes"
            dst = Path(tmp) / ".fw"
            (src / "memories").mkdir(parents=True)
            (src / "skills" / "gpu-tune").mkdir(parents=True)
            (src / "cron").mkdir(parents=True)

            (src / "config.yaml").write_text(
                "provider: anthropic\nmodel:\n  default: claude-opus-4\nchannels:\n  - cli\n  - discord\n",
                encoding="utf-8",
            )
            (src / "memories" / "notes.md").write_text("Use Q8_0 for KV cache.", encoding="utf-8")
            (src / "memories" / "log.jsonl").write_text(
                json.dumps({"id": "m1", "kind": "episodic", "topic": "triton", "content": "autotune"}) + "\n",
                encoding="utf-8",
            )
            (src / "skills" / "gpu-tune" / "SKILL.md").write_text(
                "# GPU Tune\nRun Triton autotuner.\n",
                encoding="utf-8",
            )
            (src / "cron" / "nightly.yaml").write_text(
                "id: nightly-eval\nschedule: \"0 3 * * *\"\nprompt: run fw bench\nenabled: true\n",
                encoding="utf-8",
            )

            # Dry run makes zero writes
            dry = migrate_hermes_to_fw(src, dst, dry_run=True)
            self.assertTrue(dry.dry_run)
            self.assertTrue(dry.config_migrated)
            self.assertEqual(dry.memories_migrated, 2)
            self.assertEqual(dry.skills_migrated, 1)
            self.assertEqual(dry.cron_jobs_migrated, 1)
            self.assertFalse(dst.exists())

            # Apply writes normalized artifacts
            applied = migrate_hermes_to_fw(src, dst, dry_run=False)
            self.assertFalse(applied.dry_run)
            self.assertTrue((dst / "fw-config.json").is_file())
            cfg = json.loads((dst / "fw-config.json").read_text(encoding="utf-8"))
            self.assertEqual(cfg["provider"], "anthropic")
            self.assertEqual(cfg["model"]["default"], "claude-opus-4")
            self.assertEqual(cfg["channels"], ["cli", "discord"])

            mem_lines = (dst / "memories" / "index.jsonl").read_text(encoding="utf-8").splitlines()
            self.assertEqual(len(mem_lines), 2)

            skill_text = (dst / "skills" / "gpu-tune" / "SKILL.md").read_text(encoding="utf-8")
            self.assertTrue(skill_text.startswith("---\nname: gpu-tune\n"))

            cron_data = json.loads((dst / "cron" / "jobs.json").read_text(encoding="utf-8"))
            self.assertEqual(len(cron_data["jobs"]), 1)
            self.assertEqual(cron_data["jobs"][0]["id"], "nightly-eval")
            self.assertEqual(cron_data["jobs"][0]["schedule"], "0 3 * * *")


if __name__ == "__main__":
    unittest.main()
