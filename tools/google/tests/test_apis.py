from __future__ import annotations

from pathlib import Path
from unittest import mock

import pytest

from fw_google import drive, sheets, youtube


def chain(result: object) -> mock.MagicMock:
    """A fluent mock: any call chain ends in .execute() returning `result`."""
    node = mock.MagicMock()
    node.return_value = node
    node.execute.return_value = result
    return node


def test_drive_list_paginates_and_maps() -> None:
    svc = mock.MagicMock()
    pages = [
        {
            "files": [{"id": "1", "name": "a", "mimeType": "text/plain", "size": "12"}],
            "nextPageToken": "t",
        },
        {"files": [{"id": "2", "name": "b", "mimeType": "application/pdf"}]},
    ]
    svc.files.return_value.list.return_value.execute.side_effect = pages
    files = drive.list_files(svc, "name contains 'x'", limit=5)
    assert [f.id for f in files] == ["1", "2"]
    assert files[0].size == 12
    assert files[1].size is None
    second = svc.files.return_value.list.call_args_list[1].kwargs
    assert second["pageToken"] == "t"


def test_drive_list_respects_limit() -> None:
    svc = mock.MagicMock()
    page = {
        "files": [{"id": str(i), "name": "n", "mimeType": "m"} for i in range(3)],
        "nextPageToken": "t",
    }
    svc.files.return_value.list.return_value.execute.return_value = page
    assert len(drive.list_files(svc, limit=2)) == 2


def test_drive_export_writes_file(tmp_path: Path) -> None:
    svc = mock.MagicMock()

    class FakeDownloader:
        def __init__(self, buffer: object, request: object) -> None:
            self.buffer = buffer

        def next_chunk(self) -> tuple[None, bool]:
            self.buffer.write(b"%PDF")  # type: ignore[attr-defined]
            return None, True

    with mock.patch.object(drive, "MediaIoBaseDownload", FakeDownloader):
        out = drive.export_file(svc, "fid", tmp_path / "sub" / "x.pdf", "pdf")
    assert out.read_bytes() == b"%PDF"
    svc.files.return_value.export_media.assert_called_once_with(
        fileId="fid", mimeType="application/pdf"
    )


def test_drive_export_rejects_unknown_format(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="unsupported"):
        drive.export_file(mock.MagicMock(), "fid", tmp_path / "x", "exe")


def test_sheets_read_and_write() -> None:
    svc = mock.MagicMock()
    svc.spreadsheets.return_value.values.return_value.get.return_value.execute.return_value = {
        "values": [["a", 1]]
    }
    assert sheets.read_range(svc, "sid", "A1:B1") == [["a", 1]]
    upd = svc.spreadsheets.return_value.values.return_value.update
    upd.return_value.execute.return_value = {"updatedCells": 4}
    assert sheets.write_range(svc, "sid", "A1:B2", [[1, 2], [3, 4]]) == 4
    assert upd.call_args.kwargs["valueInputOption"] == "USER_ENTERED"
    sheets.write_range(svc, "sid", "A1", [[1]], raw=True)
    assert upd.call_args.kwargs["valueInputOption"] == "RAW"


def test_youtube_channel_variants() -> None:
    svc = mock.MagicMock()
    item = {
        "id": "UC1",
        "snippet": {"title": "T", "customUrl": "@t"},
        "statistics": {"subscriberCount": "10", "videoCount": "2", "viewCount": "99"},
        "contentDetails": {"relatedPlaylists": {"uploads": "UU1"}},
    }
    svc.channels.return_value.list.return_value.execute.return_value = {"items": [item]}
    (ch,) = youtube.channel_info(svc, handle="@t")
    assert (ch.id, ch.subscribers, ch.uploads_playlist) == ("UC1", 10, "UU1")
    assert svc.channels.return_value.list.call_args.kwargs["forHandle"] == "@t"
    with pytest.raises(ValueError):
        youtube.channel_info(svc)
    with pytest.raises(ValueError):
        youtube.channel_info(svc, channel_id="a", mine=True)


def test_youtube_video_batches_by_50() -> None:
    svc = mock.MagicMock()
    lst = svc.videos.return_value.list
    lst.return_value.execute.return_value = {
        "items": [
            {
                "id": "v",
                "snippet": {
                    "title": "t",
                    "channelId": "c",
                    "publishedAt": "2026-01-01T00:00:00Z",
                },
                "contentDetails": {"duration": "PT1M"},
                "statistics": {"viewCount": "5"},
            }
        ]
    }
    out = youtube.video_metadata(svc, [f"id{i}" for i in range(51)])
    assert lst.call_count == 2
    assert len(out) == 2
    assert out[0].likes is None
    assert out[0].views == 5
