"""Tests verifying cf.toml contracts against Pydantic v2 and FinOps models."""

from __future__ import annotations

from pathlib import Path

from ay_cf_py import (
    BucketContract,
    FinOpsPolicy,
    MonthlyUsage,
    TunnelContract,
    VpnConfig,
    WorkerContract,
    estimate_monthly_cost,
    load_manifest,
)

REPO_ROOT = Path(__file__).resolve().parents[3]


def test_ay_cf_manifest_validates_with_pydantic() -> None:
    """Ensure the repository cf.toml validates across all Pydantic schemas."""
    raw = load_manifest(REPO_ROOT / "packages" / "cf-control-plane" / "cf.toml")

    finops = FinOpsPolicy.model_validate(raw["finops"])
    assert finops.r2_multipart_abort_seconds == 86_400

    workers = raw["workers"]
    assert isinstance(workers, dict)
    for cfg in workers.values():
        parsed = WorkerContract.model_validate(cfg)
        assert parsed.previews is False

    buckets = raw["buckets"]
    assert isinstance(buckets, dict)
    for cfg in buckets.values():
        parsed = BucketContract.model_validate(cfg)
        assert parsed.public_r2_dev is False

    tunnels = raw["tunnels"]
    assert isinstance(tunnels, dict)
    for cfg in tunnels.values():
        parsed = TunnelContract.model_validate(cfg)
        assert parsed.connectors >= 2

    vpn = VpnConfig.model_validate(raw["vpn"])
    assert len({e.uid for e in vpn.exit}) == len(vpn.exit)
    assert len({e.table for e in vpn.exit}) == len(vpn.exit)


def test_finops_r2_tiering_reduces_monthly_bill() -> None:
    """Verify cold archival offloading reduces R2 Paid storage charges."""
    raw = load_manifest(REPO_ROOT / "packages" / "cf-control-plane" / "cf.toml")
    policy = FinOpsPolicy.model_validate(raw["finops"])

    before = estimate_monthly_cost(
        policy,
        MonthlyUsage(
            requests=12_000_000,
            cpu_ms_total=15_000_000,
            observability_events=35_000_000,
            r2_storage_gb=68.54,
            r2_class_a_ops=800_000,
            r2_class_b_ops=14_000_000,
        ),
    )
    after = estimate_monthly_cost(
        policy,
        MonthlyUsage(
            requests=6_000_000,
            cpu_ms_total=8_000_000,
            observability_events=4_000_000,
            r2_storage_gb=8.5,
            r2_class_a_ops=400_000,
            r2_class_b_ops=3_000_000,
        ),
    )

    assert before.observability_overage_usd > 0.0
    assert after.observability_overage_usd == 0.0
    assert after.r2_storage_usd == 0.0
    assert after.total_usd < before.total_usd
