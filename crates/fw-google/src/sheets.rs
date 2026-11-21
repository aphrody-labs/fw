//! Sheets v4: read and write cell values.

use crate::{Connector, Google, Result, call};
use google_sheets4::{Sheets, api::Scope, api::UpdateValuesResponse, api::ValueRange};

impl<C: Connector> Google<C> {
    fn sheets(&self) -> Sheets<C> {
        Sheets::new(self.client.clone(), self.auth.clone())
    }

    /// `GET /v4/spreadsheets/{id}/values/{range}`, read-only scope.
    pub async fn sheets_values_get(&self, spreadsheet_id: &str, range: &str) -> Result<ValueRange> {
        let hub = self.sheets();
        call(
            "sheets",
            hub.spreadsheets()
                .values_get(spreadsheet_id, range)
                .add_scope(Scope::SpreadsheetReadonly)
                .doit()
                .await,
        )
        .await
        .map(|(_, v)| v)
    }

    /// `PUT /v4/spreadsheets/{id}/values/{range}`: overwrites `range` with `values` (rows of cells).
    /// `raw` stores the cells verbatim; otherwise they are parsed as if typed (formulas, dates).
    pub async fn sheets_values_update(
        &self,
        spreadsheet_id: &str,
        range: &str,
        values: Vec<Vec<serde_json::Value>>,
        raw: bool,
    ) -> Result<UpdateValuesResponse> {
        let body = ValueRange {
            range: Some(range.to_string()),
            major_dimension: Some("ROWS".to_string()),
            values: Some(values),
        };
        let hub = self.sheets();
        let option = if raw { "RAW" } else { "USER_ENTERED" };
        call(
            "sheets",
            hub.spreadsheets()
                .values_update(body, spreadsheet_id, range)
                .value_input_option(option)
                .add_scope(Scope::Spreadsheet)
                .doit()
                .await,
        )
        .await
        .map(|(_, v)| v)
    }
}
