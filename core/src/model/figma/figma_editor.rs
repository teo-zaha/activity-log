use serde::{Deserialize, Serialize};
use strum::EnumString;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, EnumString)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum FigmaEditor {
    Figma,
    Figjam,
    Slides,
    Buzz,
    Sites,
    Make,
}
