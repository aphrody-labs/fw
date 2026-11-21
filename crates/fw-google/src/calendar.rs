//! Calendar v3: list events.

use crate::{Connector, Google, Result, call};
use chrono::{DateTime, Utc};
use google_calendar3::{CalendarHub, api::Events, api::Scope};

/// Parameters of an events listing.
#[derive(Clone, Debug)]
pub struct EventsQuery {
    /// `primary` or a calendar id.
    pub calendar_id: String,
    /// Lower bound on the event end time; `None` for no bound.
    pub time_min: Option<DateTime<Utc>>,
    pub max_results: i32,
}

impl Default for EventsQuery {
    fn default() -> Self {
        Self {
            calendar_id: "primary".to_string(),
            time_min: None,
            max_results: 10,
        }
    }
}

impl<C: Connector> Google<C> {
    /// `GET /calendar/v3/calendars/{id}/events`, recurring events expanded, ordered by start time.
    pub async fn calendar_events(&self, query: &EventsQuery) -> Result<Events> {
        let hub = CalendarHub::new(self.client.clone(), self.auth.clone());
        let mut c = hub
            .events()
            .list(&query.calendar_id)
            .single_events(true)
            .order_by("startTime")
            .max_results(query.max_results)
            .add_scope(Scope::EventReadonly);
        if let Some(t) = &query.time_min {
            c = c.time_min(*t);
        }
        call("calendar", c.doit().await).await.map(|(_, e)| e)
    }
}
