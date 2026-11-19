use crate::model::BusyStatus;

#[derive(Debug)]
pub(crate) struct RawOutlookEvent {
    /// Outlook's id for the item: `GlobalAppointmentID` on Windows, the event's `id` on macOS.
    /// Occurrences of a series can share it.
    pub id: Option<String>,
    pub subject: Option<String>,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub organizer: Option<String>,
    pub is_organizer: Option<bool>,
    pub is_default_calendar: Option<bool>,
    pub busy_status: Option<BusyStatus>,
    /// The plain-text body, invitation included; cleaned when normalized.
    pub body: Option<String>,
}
