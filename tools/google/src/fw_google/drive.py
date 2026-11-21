"""Drive listing and export."""

from __future__ import annotations

import io
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from googleapiclient.http import MediaIoBaseDownload

FIELDS = "nextPageToken, files(id, name, mimeType, modifiedTime, size, webViewLink)"

EXPORT_MIME = {
    "pdf": "application/pdf",
    "csv": "text/csv",
    "txt": "text/plain",
    "docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
}


@dataclass(frozen=True)
class DriveFile:
    id: str
    name: str
    mime_type: str
    modified_time: str | None
    size: int | None
    web_view_link: str | None


def list_files(
    service: Any, query: str | None = None, limit: int = 50
) -> list[DriveFile]:
    """List up to `limit` files matching a Drive query (`q` syntax), newest first."""
    out: list[DriveFile] = []
    token: str | None = None
    while len(out) < limit:
        page = (
            service.files()
            .list(
                q=query,
                pageSize=min(100, limit - len(out)),
                pageToken=token,
                orderBy="modifiedTime desc",
                fields=FIELDS,
                supportsAllDrives=True,
                includeItemsFromAllDrives=True,
            )
            .execute()
        )
        for item in page.get("files", []):
            size = item.get("size")
            out.append(
                DriveFile(
                    id=item["id"],
                    name=item["name"],
                    mime_type=item["mimeType"],
                    modified_time=item.get("modifiedTime"),
                    size=int(size) if size is not None else None,
                    web_view_link=item.get("webViewLink"),
                )
            )
        token = page.get("nextPageToken")
        if not token:
            break
    return out[:limit]


def export_file(service: Any, file_id: str, dest: Path, fmt: str | None = None) -> Path:
    """Export a Google-native file to `fmt` (pdf, csv, txt, docx, xlsx, pptx) or download a binary file as is."""
    if fmt is not None:
        if fmt not in EXPORT_MIME:
            raise ValueError(
                f"unsupported export format {fmt!r}; choose one of {sorted(EXPORT_MIME)}"
            )
        request = service.files().export_media(
            fileId=file_id, mimeType=EXPORT_MIME[fmt]
        )
    else:
        request = service.files().get_media(fileId=file_id, supportsAllDrives=True)
    buffer = io.BytesIO()
    downloader = MediaIoBaseDownload(buffer, request)
    done = False
    while not done:
        _, done = downloader.next_chunk()
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(buffer.getvalue())
    return dest
