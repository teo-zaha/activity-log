use serde::Serialize;
use ts_rs::TS;

use crate::model::runtime::{part_stamp::PartStamp, shown_activity::ShownActivity};

/// What `POST /api/collect` and the desktop's `collect` answer: the activity the host installed
/// or shows, as `GET /api/activity` has it plus its `kind`, and that part's stamp.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct CollectAnswer {
    #[serde(flatten)]
    #[ts(flatten)]
    pub activity: ShownActivity,
    pub stamp: PartStamp,
}
