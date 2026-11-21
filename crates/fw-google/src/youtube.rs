//! YouTube Data API v3: channels, videos and playlists (read), video metadata update (write, gated).

use crate::{Connector, Error, Google, Result, WriteEnabled, call};
use google_youtube3::{
    YouTube, api::ChannelListResponse, api::PlaylistItemListResponse, api::PlaylistListResponse,
    api::Scope, api::Video, api::VideoListResponse, api::VideoSnippet,
};

/// Which channels to list.
#[derive(Clone, Debug)]
pub enum ChannelSelector {
    /// The authenticated user's channels.
    Mine,
    Ids(Vec<String>),
}

/// Which playlists to list.
#[derive(Clone, Debug)]
pub enum PlaylistSelector {
    Mine,
    Channel(String),
    Ids(Vec<String>),
}

/// Changes to a video's snippet; `None` leaves the field as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VideoPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category_id: Option<String>,
}

impl VideoPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.tags.is_none()
            && self.category_id.is_none()
    }
}

/// The writable part of `current` with `patch` applied. `videos.update` replaces the whole snippet,
/// so unchanged fields are copied; read-only fields (thumbnails, publish date) are left out.
pub fn apply_patch(current: &VideoSnippet, patch: &VideoPatch) -> VideoSnippet {
    VideoSnippet {
        title: patch.title.clone().or_else(|| current.title.clone()),
        description: patch
            .description
            .clone()
            .or_else(|| current.description.clone()),
        tags: patch.tags.clone().or_else(|| current.tags.clone()),
        category_id: patch
            .category_id
            .clone()
            .or_else(|| current.category_id.clone()),
        default_language: current.default_language.clone(),
        default_audio_language: current.default_audio_language.clone(),
        ..Default::default()
    }
}

fn parts(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| (*s).to_string()).collect()
}

impl<C: Connector> Google<C> {
    fn youtube(&self) -> YouTube<C> {
        YouTube::new(self.client.clone(), self.auth.clone())
    }

    /// `GET /youtube/v3/channels` (snippet, contentDetails, statistics).
    pub async fn youtube_channels(
        &self,
        selector: &ChannelSelector,
    ) -> Result<ChannelListResponse> {
        let hub = self.youtube();
        let mut c = hub
            .channels()
            .list(&parts(&["snippet", "contentDetails", "statistics"]))
            .add_scope(Scope::Readonly);
        match selector {
            ChannelSelector::Mine => c = c.mine(true),
            ChannelSelector::Ids(ids) => {
                for id in ids {
                    c = c.add_id(id);
                }
            }
        }
        call("youtube", c.doit().await).await.map(|(_, r)| r)
    }

    /// `GET /youtube/v3/videos` by id (snippet, contentDetails, statistics, status).
    pub async fn youtube_videos(&self, ids: &[String]) -> Result<VideoListResponse> {
        if ids.is_empty() {
            return Err(Error::Invalid(
                "at least one video id is required".to_string(),
            ));
        }
        let hub = self.youtube();
        let mut c = hub
            .videos()
            .list(&parts(&[
                "snippet",
                "contentDetails",
                "statistics",
                "status",
            ]))
            .add_scope(Scope::Readonly);
        for id in ids {
            c = c.add_id(id);
        }
        call("youtube", c.doit().await).await.map(|(_, r)| r)
    }

    /// `GET /youtube/v3/playlists` (snippet, contentDetails).
    pub async fn youtube_playlists(
        &self,
        selector: &PlaylistSelector,
        max: u32,
    ) -> Result<PlaylistListResponse> {
        let hub = self.youtube();
        let mut c = hub
            .playlists()
            .list(&parts(&["snippet", "contentDetails"]))
            .max_results(max)
            .add_scope(Scope::Readonly);
        match selector {
            PlaylistSelector::Mine => c = c.mine(true),
            PlaylistSelector::Channel(id) => c = c.channel_id(id),
            PlaylistSelector::Ids(ids) => {
                for id in ids {
                    c = c.add_id(id);
                }
            }
        }
        call("youtube", c.doit().await).await.map(|(_, r)| r)
    }

    /// `GET /youtube/v3/playlistItems`: the videos of a playlist (a channel's uploads playlist id is in
    /// `contentDetails.relatedPlaylists.uploads` of [`Google::youtube_channels`]).
    pub async fn youtube_playlist_items(
        &self,
        playlist_id: &str,
        max: u32,
    ) -> Result<PlaylistItemListResponse> {
        let hub = self.youtube();
        let c = hub
            .playlist_items()
            .list(&parts(&["snippet", "contentDetails"]))
            .playlist_id(playlist_id)
            .max_results(max)
            .add_scope(Scope::Readonly);
        call("youtube", c.doit().await).await.map(|(_, r)| r)
    }

    /// Updates title, description, tags or category of one video: reads the current snippet, applies
    /// `patch`, then `PUT /youtube/v3/videos?part=snippet`. Needs the explicit [`WriteEnabled`] proof.
    pub async fn youtube_video_update(
        &self,
        _write: WriteEnabled,
        video_id: &str,
        patch: &VideoPatch,
    ) -> Result<Video> {
        if patch.is_empty() {
            return Err(Error::Invalid(
                "nothing to update: give a title, description, tags or category".to_string(),
            ));
        }
        let current = self.youtube_videos(&[video_id.to_string()]).await?;
        let snippet = current
            .items
            .and_then(|v| v.into_iter().next())
            .and_then(|v| v.snippet)
            .ok_or_else(|| {
                Error::Invalid(format!("video `{video_id}` not found or has no snippet"))
            })?;
        let request = Video {
            id: Some(video_id.to_string()),
            snippet: Some(apply_patch(&snippet, patch)),
            ..Default::default()
        };
        let hub = self.youtube();
        call(
            "youtube",
            hub.videos()
                .update(request)
                .add_scope(Scope::ForceSsl)
                .doit()
                .await,
        )
        .await
        .map(|(_, v)| v)
    }
}
