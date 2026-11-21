"""`fw-google` command line: JSON on stdout, diagnostics on stderr."""

from __future__ import annotations

import argparse
import dataclasses
import json
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Any

from googleapiclient.errors import HttpError

from . import auth, drive, sheets, youtube
from .service import build_service


def _emit(value: Any) -> None:
    def default(obj: Any) -> Any:
        if dataclasses.is_dataclass(obj) and not isinstance(obj, type):
            return dataclasses.asdict(obj)
        raise TypeError(type(obj).__name__)

    json.dump(value, sys.stdout, indent=2, default=default)
    sys.stdout.write("\n")


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="fw-google", description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser(
        "whoami",
        help="show the credential source, quota project and warnings (no token printed)",
    )
    d = sub.add_parser("drive-ls", help="list Drive files")
    d.add_argument("-q", "--query", help="Drive query, e.g. \"name contains 'report'\"")
    d.add_argument("-n", "--limit", type=int, default=20)
    e = sub.add_parser("drive-export", help="export or download a Drive file")
    e.add_argument("file_id")
    e.add_argument("dest", type=Path)
    e.add_argument("--format", choices=sorted(drive.EXPORT_MIME))
    sr = sub.add_parser("sheets-read", help="read an A1 range")
    sr.add_argument("spreadsheet_id")
    sr.add_argument("range")
    sw = sub.add_parser(
        "sheets-write", help="write rows (JSON array of arrays) to an A1 range"
    )
    sw.add_argument("spreadsheet_id")
    sw.add_argument("range")
    sw.add_argument("rows", help="JSON, e.g. '[[1,2],[3,4]]'")
    sw.add_argument("--raw", action="store_true")
    yc = sub.add_parser(
        "yt-channel", help="channel metadata by --id, --handle or --mine"
    )
    yc.add_argument("--id")
    yc.add_argument("--handle")
    yc.add_argument("--mine", action="store_true")
    yv = sub.add_parser("yt-video", help="video metadata for one or more ids")
    yv.add_argument("ids", nargs="+")
    return p


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        return run(args)
    except HttpError as err:
        sys.stderr.write(f"fw-google: HTTP {err.status_code}: {err.reason}\n")
        return 1
    except RuntimeError as err:
        sys.stderr.write(f"fw-google: {err}\n")
        return 1


def run(args: argparse.Namespace) -> int:
    if args.cmd == "whoami":
        res = auth.resolve_credentials([auth.CLOUD_PLATFORM])
        _emit(
            {
                "source": res.source,
                "project": res.project,
                "warnings": list(res.warnings),
            }
        )
        return 0
    if args.cmd in {"drive-ls", "drive-export"}:
        svc = build_service("drive", "v3", [auth.DRIVE_READONLY])
        if args.cmd == "drive-ls":
            _emit(drive.list_files(svc, args.query, args.limit))
        else:
            _emit(
                {
                    "written": str(
                        drive.export_file(svc, args.file_id, args.dest, args.format)
                    )
                }
            )
    elif args.cmd.startswith("sheets-"):
        svc = build_service("sheets", "v4", [auth.SHEETS])
        if args.cmd == "sheets-read":
            _emit(sheets.read_range(svc, args.spreadsheet_id, args.range))
        else:
            _emit(
                {
                    "updatedCells": sheets.write_range(
                        svc,
                        args.spreadsheet_id,
                        args.range,
                        json.loads(args.rows),
                        raw=args.raw,
                    )
                }
            )
    else:
        svc = build_service("youtube", "v3", [auth.YOUTUBE_READONLY])
        if args.cmd == "yt-channel":
            _emit(
                youtube.channel_info(
                    svc, channel_id=args.id, handle=args.handle, mine=args.mine
                )
            )
        else:
            _emit(youtube.video_metadata(svc, args.ids))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
