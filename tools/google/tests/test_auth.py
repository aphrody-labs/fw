from __future__ import annotations

from pathlib import Path
from unittest import mock

import pytest
from google.auth.exceptions import DefaultCredentialsError

from fw_google import auth


def test_stale_gac_detected(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    missing = tmp_path / "nope.json"
    monkeypatch.setenv(auth.GAC, str(missing))
    assert auth.stale_gac() == str(missing)
    present = tmp_path / "sa.json"
    present.write_text("{}")
    monkeypatch.setenv(auth.GAC, str(present))
    assert auth.stale_gac() is None
    monkeypatch.delenv(auth.GAC)
    assert auth.stale_gac() is None


def test_adc_used_and_stale_gac_restored(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    missing = str(tmp_path / "nope.json")
    monkeypatch.setenv(auth.GAC, missing)
    seen: list[str | None] = []

    def fake_default(scopes: list[str]) -> tuple[object, str]:
        import os

        seen.append(os.environ.get(auth.GAC))
        return object(), "proj"

    with mock.patch.object(auth.google.auth, "default", fake_default):
        res = auth.resolve_credentials([auth.CLOUD_PLATFORM])
    assert res.source == "adc"
    assert res.project == "proj"
    assert seen == [None]  # the stale variable was hidden while ADC ran
    import os

    assert os.environ[auth.GAC] == missing  # and restored afterwards
    assert any("missing file" in w for w in res.warnings)


def test_fallback_to_gcloud_user_token(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv(auth.GAC, raising=False)
    calls: list[tuple[str, ...]] = []

    def fake_gcloud(*args: str) -> str:
        calls.append(args)
        return "tok" if args[0] == "auth" else "myproj"

    with (
        mock.patch.object(
            auth.google.auth, "default", side_effect=DefaultCredentialsError("none")
        ),
        mock.patch.object(auth, "gcloud", fake_gcloud),
    ):
        res = auth.resolve_credentials([auth.DRIVE_READONLY])
    assert res.source == "gcloud-user-token"
    assert res.project == "myproj"
    assert res.credentials.token == "tok"  # type: ignore[attr-defined]
    assert ("auth", "print-access-token") in calls
    assert any("ADC unavailable" in w for w in res.warnings)


def test_gcloud_missing_raises(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(auth.shutil, "which", lambda _: None)
    with pytest.raises(RuntimeError, match="not found"):
        auth.gcloud("auth", "print-access-token")
