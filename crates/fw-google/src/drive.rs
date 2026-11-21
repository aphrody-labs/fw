//! Drive v3: list files, export Google-native files to another format.

use crate::{Connector, Google, Result, call, read_body};
use google_drive3::{DriveHub, api::FileList, api::Scope};

/// Fields requested by [`Google::drive_list`] (keeps responses small).
pub const LIST_FIELDS: &str =
    "nextPageToken,files(id,name,mimeType,modifiedTime,size,parents,webViewLink,trashed)";

/// Parameters of a Drive file listing.
#[derive(Clone, Debug, Default)]
pub struct DriveListQuery {
    /// Drive search syntax, e.g. `name contains 'report' and trashed = false`.
    pub q: Option<String>,
    /// Page size (1 to 1000); the API default when `None`.
    pub page_size: Option<i32>,
    pub page_token: Option<String>,
    /// e.g. `modifiedTime desc`.
    pub order_by: Option<String>,
}

impl<C: Connector> Google<C> {
    fn drive(&self) -> DriveHub<C> {
        DriveHub::new(self.client.clone(), self.auth.clone())
    }

    /// `GET /drive/v3/files`, read-only scope.
    pub async fn drive_list(&self, query: &DriveListQuery) -> Result<FileList> {
        let hub = self.drive();
        let mut call_ = hub
            .files()
            .list()
            .param("fields", LIST_FIELDS)
            .supports_all_drives(true)
            .include_items_from_all_drives(true)
            .add_scope(Scope::Readonly);
        if let Some(q) = &query.q {
            call_ = call_.q(q);
        }
        if let Some(n) = query.page_size {
            call_ = call_.page_size(n);
        }
        if let Some(t) = &query.page_token {
            call_ = call_.page_token(t);
        }
        if let Some(o) = &query.order_by {
            call_ = call_.order_by(o);
        }
        call("drive", call_.doit().await)
            .await
            .map(|(_, list)| list)
    }

    /// `GET /drive/v3/files/{id}/export?mimeType=...`: the bytes of a Docs/Sheets/Slides file in `mime_type`.
    pub async fn drive_export(&self, file_id: &str, mime_type: &str) -> Result<Vec<u8>> {
        let hub = self.drive();
        let resp = call(
            "drive",
            hub.files()
                .export(file_id, mime_type)
                .add_scope(Scope::Readonly)
                .doit()
                .await,
        )
        .await?;
        read_body("drive", resp).await
    }
}
