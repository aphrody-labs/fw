from __future__ import annotations

import json
from unittest import mock

import pytest

from fw_google import auth, cli


def test_whoami_reports_source_without_token(
    capsys: pytest.CaptureFixture[str],
) -> None:
    fake = auth.Resolved(mock.MagicMock(), "adc", "proj", ("w",))
    with mock.patch.object(cli.auth, "resolve_credentials", return_value=fake):
        assert cli.main(["whoami"]) == 0
    assert json.loads(capsys.readouterr().out) == {
        "source": "adc",
        "project": "proj",
        "warnings": ["w"],
    }


def test_yt_video_dispatch(capsys: pytest.CaptureFixture[str]) -> None:
    svc = mock.MagicMock()
    svc.videos.return_value.list.return_value.execute.return_value = {"items": []}
    with mock.patch.object(cli, "build_service", return_value=svc) as build:
        assert cli.main(["yt-video", "abc"]) == 0
    assert build.call_args.args[:2] == ("youtube", "v3")
    assert json.loads(capsys.readouterr().out) == []


def test_sheets_write_parses_rows(capsys: pytest.CaptureFixture[str]) -> None:
    svc = mock.MagicMock()
    svc.spreadsheets.return_value.values.return_value.update.return_value.execute.return_value = {
        "updatedCells": 2
    }
    with mock.patch.object(cli, "build_service", return_value=svc):
        assert cli.main(["sheets-write", "sid", "A1:B1", "[[1,2]]"]) == 0
    assert json.loads(capsys.readouterr().out) == {"updatedCells": 2}


def test_http_error_is_reported_not_raised(capsys: pytest.CaptureFixture[str]) -> None:
    from googleapiclient.errors import HttpError

    resp = mock.MagicMock(status=403, reason="insufficient scopes")
    with mock.patch.object(cli, "build_service", side_effect=HttpError(resp, b"{}")):
        assert cli.main(["drive-ls"]) == 1
    assert "HTTP 403" in capsys.readouterr().err
