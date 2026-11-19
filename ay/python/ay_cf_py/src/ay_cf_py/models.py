"""Battle-tested Pydantic v2 and official Cloudflare SDK models for ay-cf."""

from __future__ import annotations

import ipaddress
import tomllib
from pathlib import Path
from typing import Literal

from cloudflare import Cloudflare
from pydantic import BaseModel, ConfigDict, Field, field_validator


class StrictModel(BaseModel):
    """Strict immutable base model rejecting unknown fields."""

    model_config = ConfigDict(extra="forbid", frozen=True)


class FinOpsPolicy(StrictModel):
    """Workers Paid and R2 Paid quota and target billing policies."""

    workers_included_requests: int = Field(gt=0)
    workers_overage_per_million_usd: float = Field(ge=0)
    workers_included_cpu_ms: int = Field(gt=0)
    workers_cpu_per_million_ms_usd: float = Field(ge=0)
    observability_included_events: int = Field(gt=0)
    observability_overage_per_million_usd: float = Field(ge=0)
    r2_included_gb: float = Field(ge=0)
    r2_storage_per_gb_usd: float = Field(ge=0)
    r2_included_class_a: int = Field(gt=0)
    r2_class_a_per_million_usd: float = Field(ge=0)
    r2_included_class_b: int = Field(gt=0)
    r2_class_b_per_million_usd: float = Field(ge=0)
    default_cdn_cpu_ms: int = Field(ge=10, le=30_000)
    default_api_cpu_ms: int = Field(ge=10, le=30_000)
    cdn_log_sampling_rate: float = Field(ge=0.0, le=1.0)
    api_log_sampling_rate: float = Field(ge=0.0, le=1.0)
    default_trace_sampling_rate: float = Field(ge=0.0, le=1.0)
    r2_multipart_abort_seconds: int = Field(ge=3_600, le=604_800)


class WorkerContract(StrictModel):
    """Declarative FinOps and secret-binding contract for a Cloudflare Worker."""

    profile: Literal["cdn-router", "web-router", "api-compute", "api-gateway", "webhook"]
    cpu_ms: int = Field(ge=10, le=30_000)
    log_sampling: float = Field(ge=0.0, le=1.0)
    trace_sampling: float = Field(ge=0.0, le=1.0)
    workers_dev: bool
    previews: bool
    secrets: list[str] = Field(default_factory=list)


class BucketContract(StrictModel):
    """Declarative R2 bucket policy and custom domain contract."""

    tier: Literal["hot-reusable", "dist", "site-cdn", "cold-offload"]
    custom_domains: list[str] = Field(default_factory=list)
    abort_multipart_seconds: int = Field(ge=3_600, le=604_800)
    public_r2_dev: bool = False


class TunnelContract(StrictModel):
    """Declarative cloudflared tunnel ingress contract."""

    transport: Literal["quic", "http2"]
    connectors: int = Field(ge=1)
    ingress: list[str] = Field(min_length=1)


class VpnExit(StrictModel):
    """Mullvad UID split-tunnel exit specification (STANDARD ayvpn policy)."""

    id: str = Field(min_length=2, max_length=8)
    purpose: str
    country: str = Field(min_length=2, max_length=2)
    city: str = Field(min_length=3, max_length=4)
    uid: int = Field(ge=1_000, le=60_000)
    table: int = Field(ge=51_820, le=60_000)


class VpnConfig(StrictModel):
    """Mullvad VPN cluster split-tunneling configuration."""

    provider: Literal["mullvad"]
    mode: Literal["uid-policy-routing"]
    preserve_mesh_cidr: str
    exit: list[VpnExit] = Field(min_length=1)

    @field_validator("preserve_mesh_cidr")
    @classmethod
    def validate_cidr(cls, value: str) -> str:
        """Ensure mesh CIDR parses cleanly via stdlib ipaddress."""
        ipaddress.ip_network(value, strict=False)
        return value


class MonthlyUsage(StrictModel):
    """Observed monthly Cloudflare usage inputs for FinOps estimation."""

    requests: int = Field(ge=0)
    cpu_ms_total: int = Field(ge=0)
    observability_events: int = Field(ge=0)
    r2_storage_gb: float = Field(ge=0.0)
    r2_class_a_ops: int = Field(ge=0)
    r2_class_b_ops: int = Field(ge=0)


class FinOpsEstimate(StrictModel):
    """Breakdown of estimated monthly Cloudflare spend in USD."""

    workers_base_usd: float
    workers_overage_usd: float
    observability_overage_usd: float
    r2_storage_usd: float
    r2_ops_usd: float
    total_usd: float


def load_manifest(path: Path) -> dict[str, object]:
    """Load cf.toml with stdlib tomllib."""
    with path.open("rb") as fh:
        return tomllib.load(fh)


def estimate_monthly_cost(policy: FinOpsPolicy, usage: MonthlyUsage) -> FinOpsEstimate:
    """Compute itemized monthly Cloudflare cost under Workers Paid + R2 Paid."""
    extra_req = max(0, usage.requests - policy.workers_included_requests)
    extra_cpu = max(0, usage.cpu_ms_total - policy.workers_included_cpu_ms)
    workers_overage = (extra_req / 1_000_000.0) * policy.workers_overage_per_million_usd + (
        extra_cpu / 1_000_000.0
    ) * policy.workers_cpu_per_million_ms_usd

    extra_obs = max(0, usage.observability_events - policy.observability_included_events)
    obs_overage = (extra_obs / 1_000_000.0) * policy.observability_overage_per_million_usd

    billable_gb = max(0.0, usage.r2_storage_gb - policy.r2_included_gb)
    r2_storage = billable_gb * policy.r2_storage_per_gb_usd

    extra_a = max(0, usage.r2_class_a_ops - policy.r2_included_class_a)
    extra_b = max(0, usage.r2_class_b_ops - policy.r2_included_class_b)
    r2_ops = (extra_a / 1_000_000.0) * policy.r2_class_a_per_million_usd + (
        extra_b / 1_000_000.0
    ) * policy.r2_class_b_per_million_usd

    base = 5.0
    total = round(base + workers_overage + obs_overage + r2_storage + r2_ops, 4)
    return FinOpsEstimate(
        workers_base_usd=base,
        workers_overage_usd=round(workers_overage, 4),
        observability_overage_usd=round(obs_overage, 4),
        r2_storage_usd=round(r2_storage, 4),
        r2_ops_usd=round(r2_ops, 4),
        total_usd=total,
    )


def build_cloudflare_client(api_token: str | None = None) -> Cloudflare:
    """Instantiate the official Cloudflare Python SDK client."""
    return Cloudflare(api_token=api_token) if api_token else Cloudflare()
