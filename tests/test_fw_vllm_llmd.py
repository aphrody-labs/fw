"""Unit tests for Fw vLLM plugin, llm-d EPP scorer, and migration CLI."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import json
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from typing import Any
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent


def _load_module(name: str, rel_path: str) -> Any:
    path = REPO_ROOT / rel_path
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load module spec for {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


fw_vllm_plugin = _load_module("fw_vllm_plugin", "plugins/vllm-fw/fw_vllm_plugin.py")
fw_llmd_scorer = _load_module("fw_llmd_scorer", "plugins/llmd-fw/fw_llmd_scorer.py")
migrate_mod = _load_module(
    "migrate_vllm_llmd_to_fw",
    "core/py/fw_py_ai/migrate_vllm_llmd_to_fw.py",
)


class TestVllmFwPlugin(unittest.TestCase):
    def setUp(self) -> None:
        fw_vllm_plugin.reset_registry()

    def tearDown(self) -> None:
        fw_vllm_plugin.reset_registry()

    def test_plugin_toml_manifest_valid(self) -> None:
        manifest_path = REPO_ROOT / "plugins/vllm-fw/plugin.toml"
        data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(data["plugin"]["name"], "vllm-fw")
        self.assertEqual(data["plugin"]["protocol"], "vllm.general_plugins")
        self.assertEqual(data["plugin"]["entrypoint"], "fw_vllm_plugin:register")
        self.assertEqual(data["plugin"]["dependencies"], [])
        self.assertEqual(data["vllm"]["kv_connector_abi"], "KVConnectorBase_V1")
        self.assertTrue(data["fw"]["disaggregated_pd"])

    def test_register_idempotency_and_allowlist_gating(self) -> None:
        # Explicit empty allowlist blocks registration (matches vllm/plugins/__init__.py)
        blocked = fw_vllm_plugin.register(env={"VLLM_PLUGINS": ""})
        self.assertFalse(blocked["registered"])
        self.assertTrue(blocked["skipped"])
        self.assertFalse(fw_vllm_plugin.is_registered())

        # Unrelated allowlist also skips
        skipped = fw_vllm_plugin.register(env={"VLLM_PLUGINS": "other_plugin"})
        self.assertFalse(skipped["registered"])

        # Matching allowlist registers once; second call is idempotent
        first = fw_vllm_plugin.register(
            env={"VLLM_PLUGINS": f"foo,{fw_vllm_plugin.PLUGIN_NAME}"}
        )
        self.assertTrue(first["registered"])
        self.assertFalse(first["already_registered"])
        self.assertEqual(first["registration_count"], 1)
        self.assertTrue(fw_vllm_plugin.is_registered())

        second = fw_vllm_plugin.register(env={})
        self.assertTrue(second["registered"])
        self.assertTrue(second["already_registered"])
        self.assertEqual(second["registration_count"], 1)
        self.assertIsNotNone(fw_vllm_plugin.get_connector())

    def test_prefix_key_matches_fw_cluster_rust_sha256_contract(self) -> None:
        prompt = "System prompt: Fw v3.0 ggml-rpc hybrid cluster"
        expected = hashlib.sha256(prompt.encode("utf-8")[:128]).hexdigest()[:16]
        self.assertEqual(fw_vllm_plugin.fw_prefix_key(prompt), expected)
        self.assertEqual(len(expected), 16)

        # Suffix beyond 128 bytes must produce identical prefix key
        long_a = ("A" * 128) + "suffix-one"
        long_b = ("A" * 128) + "suffix-two"
        self.assertEqual(
            fw_vllm_plugin.fw_prefix_key(long_a),
            fw_vllm_plugin.fw_prefix_key(long_b),
        )

    def test_kv_connector_v1_lifecycle_and_fw_bench_bridge(self) -> None:
        cfg = fw_vllm_plugin.KVConnectorConfig.from_vllm_json(
            json.dumps(
                {
                    "kv_connector": "NixlConnector",
                    "kv_role": "kv_producer",
                    "kv_connector_extra_config": {
                        "block_size": 4,
                        "node_id": "fixture-node-b",
                    },
                }
            )
        )
        self.assertEqual(cfg.transport, "nixl")
        self.assertEqual(cfg.kv_role, fw_vllm_plugin.KVRole.KV_PRODUCER)

        conn = fw_vllm_plugin.FwKVConnectorV1(cfg)
        prompt = "Explain Fw PagedAttention and NIXL KV transfer"
        tokens = [10, 20, 30, 40, 50, 60, 70, 80]
        hashes = fw_vllm_plugin.compute_paged_block_hashes(tokens, block_size=4)
        self.assertEqual(len(hashes), 2)

        # Cold miss
        matched, load_async = conn.get_num_new_matched_tokens("req-1", tokens, prompt=prompt)
        self.assertEqual(matched, 0)
        self.assertFalse(load_async)

        # Save KV layers & finish request
        conn.update_state_after_alloc("req-1", hashes, 0)
        conn.save_kv_layer("req-1", 0, hashes, prompt=prompt)
        conn.save_kv_layer("req-1", 1, hashes, prompt=prompt)
        self.assertEqual(conn.wait_for_save("req-1"), 2)

        delay_free, params = conn.request_finished("req-1", hashes, prompt=prompt)
        self.assertTrue(delay_free)
        assert params is not None
        self.assertTrue(params["do_remote_prefill"])
        self.assertEqual(params["remote_node_id"], "fixture-node-b")

        events = conn.take_events()
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["type"], "BlockStored")
        self.assertEqual(conn.take_events(), [])

        # Warm hit on second request with same prefix
        matched2, _ = conn.get_num_new_matched_tokens("req-2", tokens, prompt=prompt)
        self.assertEqual(matched2, 8)
        xfer = conn.start_load_kv("req-2", hashes)
        self.assertEqual(xfer.status, "completed")
        self.assertEqual(xfer.matched_tokens, 8)
        self.assertTrue(conn.wait_for_layer_load("req-2", 0))

        sending, recving = conn.get_transfer_results()
        self.assertIn("req-1", sending)
        self.assertIn("req-2", recving)

        # Preemption cleanup
        conn.update_state_after_alloc("req-3", hashes, 8)
        self.assertEqual(conn.handle_preemptions(["req-3"]), 1)
        self.assertEqual(conn.take_events()[0]["type"], "BlockRemoved")

        # Export fw-cluster prefix index & dry-run fw bench
        kv_index = conn.export_cluster_kv_index()
        self.assertEqual(kv_index[fw_vllm_plugin.fw_prefix_key(prompt)], "fixture-node-b")
        bench = conn.run_fw_bench(model="Qwen/Qwen3-32B", dry_run=True)
        self.assertTrue(bench["dry_run"])
        self.assertEqual(bench["returncode"], 0)
        self.assertEqual(bench["payload"]["registered_blocks"], 2)


class TestLlmdFwScorer(unittest.TestCase):
    def test_plugin_toml_manifest_valid(self) -> None:
        manifest_path = REPO_ROOT / "plugins/llmd-fw/plugin.toml"
        data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(data["plugin"]["name"], "llmd-fw")
        self.assertEqual(data["plugin"]["protocol"], "llm-d.ai/v1/EndpointPickerConfig")
        self.assertEqual(data["plugin"]["entrypoint"], "fw_llmd_scorer:score_endpoints")
        self.assertEqual(data["llmd"]["api_version"], "llm-d.ai/v1")
        self.assertIn("token-load-scorer", data["llmd"]["scorer_plugins"])

    def test_score_endpoints_prefix_affinity_and_load_balancing(self) -> None:
        prompt = "System prompt: Fw llm-d prefix cache routing benchmark"
        pkey = fw_llmd_scorer.fw_prefix_key(prompt)
        endpoints = [
            {
                "id": "gpu-cold-idle",
                "endpoint": "http://192.0.2.21:8000",
                "role": "prefill",
                "healthy": True,
                "inflight": 0,
                "kv_cache_usage": 0.1,
                "prefill_score": 1.4,
                "cached_prefixes": [],
            },
            {
                "id": "gpu-warm-hit",
                "endpoint": "http://192.0.2.22:8000",
                "role": "prefill",
                "healthy": True,
                "inflight": 1,
                "kv_cache_usage": 0.25,
                "prefill_score": 1.2,
                "cached_prefixes": [pkey],
            },
            {
                "id": "gpu-unhealthy",
                "endpoint": "http://192.0.2.23:8000",
                "role": "prefill",
                "healthy": False,
                "inflight": 0,
                "cached_prefixes": [pkey],
            },
        ]
        scored = fw_llmd_scorer.score_endpoints(prompt, endpoints, role="prefill")
        self.assertEqual(scored[0].endpoint_id, "gpu-warm-hit")
        self.assertTrue(scored[0].prefix_hit)
        self.assertTrue(scored[0].eligible)
        self.assertEqual(scored[-1].endpoint_id, "gpu-unhealthy")
        self.assertFalse(scored[-1].eligible)
        self.assertEqual(scored[-1].reason, "unhealthy")

    def test_select_pd_endpoints_disaggregated_with_vps_and_webgpu(self) -> None:
        prompt = "System prompt: Fw v4.2 disaggregated P/D routing"
        pkey = fw_llmd_scorer.fw_prefix_key(prompt)
        endpoints = [
            fw_llmd_scorer.EndpointState(
                id="gpu-prefill-1",
                endpoint="http://192.0.2.21:8000",
                role="prefill",
                prefill_score=1.6,
                decode_score=0.2,
                cached_prefixes=(pkey,),
            ),
            fw_llmd_scorer.EndpointState(
                id="gpu-decode-1",
                endpoint="http://192.0.2.22:8200",
                role="decode",
                inflight=4,
                kv_cache_usage=0.7,
                decode_score=1.3,
            ),
            fw_llmd_scorer.EndpointState(
                id="gpu-decode-2",
                endpoint="http://192.0.2.23:8200",
                role="decode",
                inflight=0,
                kv_cache_usage=0.1,
                decode_score=1.5,
            ),
            fw_llmd_scorer.EndpointState(
                id="fixture-proxy-a",
                endpoint="http://192.0.2.11:8080",
                role="proxy",
                kind="VpsProxyLbStorageCpu",
            ),
            fw_llmd_scorer.EndpointState(
                id="user-webgpu-peer-1",
                endpoint="webrtc://peer-1/fw-compute",
                role="webgpu_draft",
                kind="UserWebGpuFpu",
            ),
        ]
        decision = fw_llmd_scorer.select_pd_endpoints(
            prompt,
            endpoints,
            prefer_user_webgpu_draft=True,
        )
        self.assertEqual(decision.prefix_hash, pkey)
        self.assertTrue(decision.kv_cache_hit)
        self.assertTrue(decision.disaggregated)
        self.assertEqual(decision.prefill_node, "gpu-prefill-1")
        self.assertEqual(decision.decode_node, "gpu-decode-2")
        self.assertEqual(decision.proxy_vps, "fixture-proxy-a")
        self.assertEqual(decision.draft_user_webgpu, "user-webgpu-peer-1")

    def test_select_pd_single_node_fallback(self) -> None:
        endpoints = [
            fw_llmd_scorer.EndpointState(
                id="gpu-solo",
                endpoint="http://127.0.0.1:4379",
                role="both",
            )
        ]
        decision = fw_llmd_scorer.select_pd_endpoints("hello", endpoints)
        self.assertFalse(decision.disaggregated)
        self.assertEqual(decision.prefill_node, "gpu-solo")
        self.assertEqual(decision.decode_node, "gpu-solo")
        self.assertEqual(decision.proxy_vps, "local-direct")


class TestMigrateVllmLlmdToFw(unittest.TestCase):
    def test_migrate_vllm_cli_flags_and_parity_gaps(self) -> None:
        cli = (
            "vllm serve Qwen/Qwen3-32B --host 0.0.0.0 --port 8000 "
            "--tensor-parallel-size 2 --enable-prefix-caching --enable-lora "
            '--kv-transfer-config \'{"kv_connector":"NixlConnector","kv_role":"kv_producer"}\''
        )
        res = migrate_mod.migrate_vllm_to_fw(cli, apply=False)
        self.assertFalse(res.applied)
        self.assertEqual(res.written_files, [])
        self.assertEqual(res.cluster_config["cluster_name"], "fw-migrated-vllm")
        self.assertEqual(res.cluster_config["migration"]["models"], ["Qwen/Qwen3-32B"])
        gpu_node = res.cluster_config["nodes"][0]
        self.assertEqual(gpu_node["endpoint"], "http://127.0.0.1:8000")
        self.assertEqual(gpu_node["vram_gb"], 0)
        self.assertIsNone(gpu_node["rpc_addr"])
        self.assertGreater(gpu_node["prefill_score"], gpu_node["decode_score"])

        gap_map = {g.feature: g.fw_status for g in res.parity_gaps}
        self.assertEqual(gap_map["enable_prefix_caching"], "mapped")
        self.assertEqual(gap_map["kv_transfer_config"], "partial")
        self.assertEqual(gap_map["enable_lora"], "unsupported_in_fw")

    def test_migrate_vllm_docker_compose_pd_stack(self) -> None:
        compose = """
services:
  vllm-prefill:
    image: vllm/vllm-openai:latest
    ports:
      - "8000:8000"
    command: --model Qwen/Qwen3-32B --port 8000 --enable-prefix-caching --kv-transfer-config '{"kv_connector":"NixlConnector","kv_role":"kv_producer"}'
  vllm-decode:
    image: vllm/vllm-openai:latest
    ports:
      - "8200:8200"
    command: --model Qwen/Qwen3-32B --port 8200 --kv-transfer-config '{"kv_connector":"NixlConnector","kv_role":"kv_consumer"}'
"""
        res = migrate_mod.migrate_vllm_to_fw(compose, apply=False)
        self.assertEqual(res.cluster_config["migration"]["input_format"], "docker-compose")
        node_ids = [n["id"] for n in res.cluster_config["nodes"]]
        self.assertIn("vllm-prefill", node_ids)
        self.assertIn("vllm-decode", node_ids)
        self.assertIn("local-proxy", node_ids)

    def test_migrate_llmd_helm_values_and_inference_pool(self) -> None:
        llmd_yaml = """
apiVersion: inference.networking.k8s.io/v1
kind: InferencePool
metadata:
  name: pd-disagg-pool
spec:
  targetPorts:
    - number: 8000
---
router:
  epp:
    replicas: 1
    pluginsCustomConfig:
      pd-config.yaml: |
        apiVersion: llm-d.ai/v1
        kind: EndpointPickerConfig
        plugins:
          - type: token-producer
            parameters:
              modelName: Qwen/Qwen3-32B
          - type: always-disagg-pd-decider
          - type: prefix-cache-affinity-filter
            parameters:
              peakPrefillThroughput: 33821
          - type: token-load-scorer
          - type: active-request-scorer
        schedulingProfiles:
          - name: prefill
            plugins:
              - pluginRef: prefix-cache-affinity-filter
              - pluginRef: token-load-scorer
          - name: decode
            plugins:
              - pluginRef: active-request-scorer
"""
        res = migrate_mod.migrate_llmd_to_fw(llmd_yaml, apply=False)
        mig = res.cluster_config["migration"]
        self.assertTrue(mig["disaggregated_pd"])
        self.assertEqual(mig["model"], "Qwen/Qwen3-32B")
        self.assertEqual(mig["peak_prefill_throughput"], 33821.0)
        self.assertIn("prefill", mig["scheduling_profiles"])
        self.assertIn("decode", mig["scheduling_profiles"])
        node_ids = [n["id"] for n in res.cluster_config["nodes"]]
        self.assertIn("llmd-prefill-1", node_ids)
        self.assertIn("llmd-decode-1", node_ids)

        gap_map = {g.feature: g.fw_status for g in res.parity_gaps}
        self.assertEqual(gap_map["pd-disaggregation (prefill/decode profiles)"], "mapped")
        self.assertEqual(gap_map["token-producer (/v1/*/render)"], "unsupported_in_fw")
        self.assertEqual(
            gap_map["InferencePool (Gateway API Inference Extension)"], "partial"
        )

    def test_cli_dry_run_and_apply_roundtrip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            src_json = tmp_path / "vllm_config.json"
            src_json.write_text(
                json.dumps(
                    {
                        "node_id": "gpu-1-rtx4070",
                        "model": "Qwen/Qwen3-8B",
                        "host": "127.0.0.1",
                        "port": 8000,
                        "tensor_parallel_size": 1,
                        "enable_prefix_caching": True,
                    }
                ),
                encoding="utf-8",
            )
            out_dir = tmp_path / "out"

            # Dry-run writes nothing
            buf = io.StringIO()
            with mock.patch.object(sys, "stdout", buf):
                rc = migrate_mod.main(
                    ["vllm", "--source", str(src_json), "--target", str(out_dir), "--dry-run"]
                )
            self.assertEqual(rc, 0)
            self.assertFalse((out_dir / "fw-cluster.json").exists())
            dry_payload = json.loads(buf.getvalue())
            self.assertFalse(dry_payload["applied"])

            # Apply writes fw-cluster.json + fw-parity-report.json
            buf2 = io.StringIO()
            with mock.patch.object(sys, "stdout", buf2):
                rc2 = migrate_mod.main(
                    ["vllm", "--source", str(src_json), "--target", str(out_dir), "--apply"]
                )
            self.assertEqual(rc2, 0)
            apply_payload = json.loads(buf2.getvalue())
            self.assertTrue(apply_payload["applied"])
            cluster_file = out_dir / "fw-cluster.json"
            parity_file = out_dir / "fw-parity-report.json"
            self.assertTrue(cluster_file.exists())
            self.assertTrue(parity_file.exists())
            loaded_cluster = json.loads(cluster_file.read_text(encoding="utf-8"))
            self.assertEqual(loaded_cluster["nodes"][0]["id"], "gpu-1-rtx4070")


if __name__ == "__main__":
    unittest.main()
