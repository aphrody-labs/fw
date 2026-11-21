"""YouTube Data API v3 metadata (read-only)."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class Channel:
    id: str
    title: str
    handle: str | None
    subscribers: int | None
    videos: int | None
    views: int | None
    uploads_playlist: str | None


@dataclass(frozen=True)
class Video:
    id: str
    title: str
    channel_id: str
    published_at: str
    duration: str | None
    views: int | None
    likes: int | None


def _int(value: Any) -> int | None:
    return int(value) if value is not None else None


def channel_info(
    service: Any,
    *,
    channel_id: str | None = None,
    handle: str | None = None,
    mine: bool = False,
) -> list[Channel]:
    """Look a channel up by id, `@handle`, or the authorised user (`mine`; needs user OAuth, not a service account)."""
    if sum(bool(x) for x in (channel_id, handle, mine)) != 1:
        raise ValueError("pass exactly one of channel_id, handle, mine")
    params: dict[str, Any] = {"part": "snippet,statistics,contentDetails"}
    if channel_id:
        params["id"] = channel_id
    elif handle:
        params["forHandle"] = handle
    else:
        params["mine"] = True
    res = service.channels().list(**params).execute()
    out: list[Channel] = []
    for item in res.get("items", []):
        stats = item.get("statistics", {})
        out.append(
            Channel(
                id=item["id"],
                title=item["snippet"]["title"],
                handle=item["snippet"].get("customUrl"),
                subscribers=_int(stats.get("subscriberCount")),
                videos=_int(stats.get("videoCount")),
                views=_int(stats.get("viewCount")),
                uploads_playlist=item.get("contentDetails", {})
                .get("relatedPlaylists", {})
                .get("uploads"),
            )
        )
    return out


def video_metadata(service: Any, video_ids: list[str]) -> list[Video]:
    """Fetch metadata for up to 50 video ids per request (the API limit), batching larger inputs."""
    out: list[Video] = []
    for start in range(0, len(video_ids), 50):
        batch = video_ids[start : start + 50]
        res = (
            service.videos()
            .list(part="snippet,contentDetails,statistics", id=",".join(batch))
            .execute()
        )
        for item in res.get("items", []):
            stats = item.get("statistics", {})
            out.append(
                Video(
                    id=item["id"],
                    title=item["snippet"]["title"],
                    channel_id=item["snippet"]["channelId"],
                    published_at=item["snippet"]["publishedAt"],
                    duration=item.get("contentDetails", {}).get("duration"),
                    views=_int(stats.get("viewCount")),
                    likes=_int(stats.get("likeCount")),
                )
            )
    return out
