//! Docs v1: fetch a document and flatten it to plain text.

use crate::{Connector, Google, Result, call};
use google_docs1::{Docs, api::Document, api::Scope, api::StructuralElement};

/// The document's text: paragraphs in order, table cells separated by tabs and rows by newlines.
pub fn plain_text(doc: &Document) -> String {
    let mut out = String::new();
    if let Some(body) = &doc.body {
        walk(body.content.as_deref().unwrap_or_default(), &mut out);
    }
    out
}

fn walk(elements: &[StructuralElement], out: &mut String) {
    for el in elements {
        if let Some(p) = &el.paragraph {
            for e in p.elements.as_deref().unwrap_or_default() {
                if let Some(t) = e.text_run.as_ref().and_then(|t| t.content.as_deref()) {
                    out.push_str(t);
                }
            }
        } else if let Some(t) = &el.table {
            for row in t.table_rows.as_deref().unwrap_or_default() {
                let cells = row.table_cells.as_deref().unwrap_or_default();
                for (i, cell) in cells.iter().enumerate() {
                    let mut text = String::new();
                    walk(cell.content.as_deref().unwrap_or_default(), &mut text);
                    out.push_str(text.trim_end_matches('\n'));
                    out.push(if i + 1 == cells.len() { '\n' } else { '\t' });
                }
            }
        } else if let Some(toc) = &el.table_of_contents {
            walk(toc.content.as_deref().unwrap_or_default(), out);
        }
    }
}

impl<C: Connector> Google<C> {
    /// `GET /v1/documents/{id}`, read-only scope.
    pub async fn docs_get(&self, document_id: &str) -> Result<Document> {
        let hub = Docs::new(self.client.clone(), self.auth.clone());
        call(
            "docs",
            hub.documents()
                .get(document_id)
                .add_scope(Scope::DocumentReadonly)
                .doit()
                .await,
        )
        .await
        .map(|(_, d)| d)
    }

    /// The document flattened by [`plain_text`].
    pub async fn docs_text(&self, document_id: &str) -> Result<String> {
        Ok(plain_text(&self.docs_get(document_id).await?))
    }
}
