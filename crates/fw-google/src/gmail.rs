//! Gmail v1: list messages and create drafts. Nothing here sends mail.

use crate::{Connector, Error, Google, Result, call};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use google_gmail1::{Gmail, api::Draft, api::Message, api::Scope};
use serde::Serialize;
use std::io::Cursor;

/// A draft to create.
#[derive(Clone, Debug, Default)]
pub struct DraftSpec {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
}

/// One row of a message listing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MessageSummary {
    pub id: String,
    pub thread_id: Option<String>,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub date: Option<String>,
    pub snippet: Option<String>,
}

fn header_value(field: &str, v: &str) -> Result<()> {
    if v.contains(['\r', '\n']) {
        return Err(Error::Invalid(format!(
            "{field} must not contain line breaks"
        )));
    }
    Ok(())
}

/// RFC 2047 encoded-word for non-ASCII subjects, verbatim otherwise.
fn encode_subject(subject: &str) -> String {
    if subject.is_ascii() {
        subject.to_string()
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(subject.as_bytes()))
    }
}

/// The RFC 822 message of a draft: UTF-8 text, base64 transfer encoding, CRLF line ends.
/// Rejects header injection (line breaks in addresses or subject) and an empty recipient list.
pub fn rfc822_draft(spec: &DraftSpec) -> Result<Vec<u8>> {
    if spec.to.is_empty() {
        return Err(Error::Invalid(
            "a draft needs at least one recipient".to_string(),
        ));
    }
    for a in spec.to.iter().chain(&spec.cc) {
        header_value("address", a)?;
        if a.trim().is_empty() || !a.contains('@') {
            return Err(Error::Invalid(format!("`{a}` is not an email address")));
        }
    }
    header_value("subject", &spec.subject)?;
    let mut out = String::new();
    out.push_str(&format!("To: {}\r\n", spec.to.join(", ")));
    if !spec.cc.is_empty() {
        out.push_str(&format!("Cc: {}\r\n", spec.cc.join(", ")));
    }
    out.push_str(&format!("Subject: {}\r\n", encode_subject(&spec.subject)));
    out.push_str("MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    let encoded = STANDARD.encode(spec.body.as_bytes());
    for chunk in encoded.as_bytes().chunks(76) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push_str("\r\n");
    }
    Ok(out.into_bytes())
}

fn header(msg: &Message, name: &str) -> Option<String> {
    msg.payload
        .as_ref()?
        .headers
        .as_ref()?
        .iter()
        .find(|h| {
            h.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| h.value.clone())
}

pub(crate) fn summarize(msg: &Message) -> MessageSummary {
    MessageSummary {
        id: msg.id.clone().unwrap_or_default(),
        thread_id: msg.thread_id.clone(),
        subject: header(msg, "Subject"),
        from: header(msg, "From"),
        date: header(msg, "Date"),
        snippet: msg.snippet.clone(),
    }
}

impl<C: Connector> Google<C> {
    fn gmail(&self) -> Gmail<C> {
        Gmail::new(self.client.clone(), self.auth.clone())
    }

    /// Lists up to `max` messages of the mailbox matching `query` (Gmail search syntax), with
    /// subject, sender, date and snippet (one metadata request per message).
    pub async fn gmail_list(&self, query: Option<&str>, max: u32) -> Result<Vec<MessageSummary>> {
        let hub = self.gmail();
        let mut list = hub
            .users()
            .messages_list("me")
            .max_results(max)
            .add_scope(Scope::Readonly);
        if let Some(q) = query {
            list = list.q(q);
        }
        let (_, resp) = call("gmail", list.doit().await).await?;
        let mut out = Vec::new();
        for m in resp.messages.unwrap_or_default() {
            let Some(id) = m.id else { continue };
            let get = hub
                .users()
                .messages_get("me", &id)
                .format("metadata")
                .add_metadata_headers("Subject")
                .add_metadata_headers("From")
                .add_metadata_headers("Date")
                .add_scope(Scope::Readonly);
            let (_, full) = call("gmail", get.doit().await).await?;
            out.push(summarize(&full));
        }
        Ok(out)
    }

    /// Creates a draft in the mailbox (`POST /upload/gmail/v1/users/me/drafts`). The draft is never
    /// sent; there is no send operation in this crate.
    pub async fn gmail_draft_create(&self, spec: &DraftSpec) -> Result<Draft> {
        let raw = rfc822_draft(spec)?;
        let hub = self.gmail();
        let mime = "message/rfc822"
            .parse()
            .map_err(|_| Error::Invalid("mime".to_string()))?;
        let r = hub
            .users()
            .drafts_create(Draft::default(), "me")
            .add_scope(Scope::Compose)
            .upload(Cursor::new(raw), mime)
            .await;
        call("gmail", r).await.map(|(_, d)| d)
    }
}
