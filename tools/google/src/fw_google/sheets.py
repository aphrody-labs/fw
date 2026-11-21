"""Sheets read and write."""

from __future__ import annotations

from typing import Any

Cell = str | int | float | bool | None


def read_range(service: Any, spreadsheet_id: str, a1: str) -> list[list[Cell]]:
    """Read an A1 range as rows of cell values."""
    res = (
        service.spreadsheets()
        .values()
        .get(spreadsheetId=spreadsheet_id, range=a1)
        .execute()
    )
    return list(res.get("values", []))


def write_range(
    service: Any,
    spreadsheet_id: str,
    a1: str,
    rows: list[list[Cell]],
    *,
    raw: bool = False,
) -> int:
    """Overwrite an A1 range with `rows`; returns the number of updated cells.

    Values are parsed as if typed in the UI unless `raw` is set.
    """
    res = (
        service.spreadsheets()
        .values()
        .update(
            spreadsheetId=spreadsheet_id,
            range=a1,
            valueInputOption="RAW" if raw else "USER_ENTERED",
            body={"values": rows},
        )
        .execute()
    )
    return int(res.get("updatedCells", 0))
