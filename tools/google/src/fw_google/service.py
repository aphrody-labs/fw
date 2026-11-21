"""Build googleapiclient services from resolved credentials."""

from __future__ import annotations

from typing import Any

from googleapiclient.discovery import build

from .auth import Resolved, resolve_credentials


def build_service(
    name: str, version: str, scopes: list[str], resolved: Resolved | None = None
) -> Any:
    """Return a discovery client for `name`/`version`; adds the quota project header when one is known."""
    res = resolved or resolve_credentials(scopes)
    creds = res.credentials
    if res.project and hasattr(creds, "with_quota_project"):
        creds = creds.with_quota_project(res.project)
    return build(name, version, credentials=creds, cache_discovery=False)
