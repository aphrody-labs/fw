"""Credential discovery: Application Default Credentials first, the gcloud user token as fallback.

A GOOGLE_APPLICATION_CREDENTIALS value that points at a missing file breaks ADC for every client library.
`resolve_credentials` ignores such a value (and reports it) instead of failing, so a stale environment variable
does not hide working user credentials.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

import google.auth
from google.auth.credentials import Credentials
from google.auth.exceptions import DefaultCredentialsError
from google.oauth2.credentials import Credentials as UserCredentials

DRIVE_READONLY = "https://www.googleapis.com/auth/drive.readonly"
SHEETS = "https://www.googleapis.com/auth/spreadsheets"
YOUTUBE_READONLY = "https://www.googleapis.com/auth/youtube.readonly"
CLOUD_PLATFORM = "https://www.googleapis.com/auth/cloud-platform"

GAC = "GOOGLE_APPLICATION_CREDENTIALS"
QUOTA_ENV = "FW_GOOGLE_QUOTA_PROJECT"


@dataclass(frozen=True)
class Resolved:
    credentials: Credentials
    source: str
    project: str | None
    warnings: tuple[str, ...] = ()


def stale_gac() -> str | None:
    """Return the GOOGLE_APPLICATION_CREDENTIALS path when it is set but the file does not exist."""
    path = os.environ.get(GAC)
    if path and not Path(path).is_file():
        return path
    return None


def gcloud(*args: str) -> str:
    """Run gcloud and return stripped stdout; raise RuntimeError when gcloud is missing or fails."""
    binary = shutil.which("gcloud")
    if binary is None:
        raise RuntimeError("gcloud not found on PATH")
    done = subprocess.run(
        [binary, *args], capture_output=True, text=True, check=False, timeout=60
    )
    if done.returncode != 0:
        tail = done.stderr.strip().splitlines()[-1:] or ["no stderr"]
        raise RuntimeError(f"gcloud {args[0]} failed: {tail[0]}")
    return done.stdout.strip()


def gcloud_project() -> str | None:
    """Quota project: CLOUDSDK_CORE_PROJECT, then the active gcloud configuration, then GOOGLE_CLOUD_PROJECT."""
    try:
        value = gcloud("config", "get-value", "project")
    except RuntimeError:
        value = ""
    return value or os.environ.get("GOOGLE_CLOUD_PROJECT") or None


def resolve_credentials(scopes: list[str]) -> Resolved:
    """Resolve credentials for `scopes`.

    ADC is tried first. When ADC is unavailable the active gcloud user token is used; that token only carries the
    scopes gcloud was authorised with, so Drive, Sheets and YouTube calls need `gcloud auth application-default
    login --scopes=...` with an OAuth client (see tools/google/README.md).
    """
    warnings: list[str] = []
    stale = stale_gac()
    saved = os.environ.pop(GAC, None) if stale else None
    if stale:
        warnings.append(f"{GAC} points at a missing file ({stale}); ignored")
    try:
        try:
            creds, project = google.auth.default(scopes=scopes)
            return Resolved(
                creds,
                "adc",
                os.environ.get(QUOTA_ENV) or project or gcloud_project(),
                tuple(warnings),
            )
        except DefaultCredentialsError as err:
            warnings.append(f"ADC unavailable: {err}")
    finally:
        if saved is not None:
            os.environ[GAC] = saved
    token = gcloud("auth", "print-access-token")
    return Resolved(
        UserCredentials(token=token),
        "gcloud-user-token",
        gcloud_project(),
        tuple(warnings),
    )
