use serde::Deserialize;

#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
#[derive(Debug, Deserialize)]
pub(crate) struct RawSystemEvent {
    pub id: i32,
    pub ts: String,
}
