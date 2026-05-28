use serde::{Deserialize, Serialize};

#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RawSystemEvent {
    pub id: i32,
    pub ts: String,
}
