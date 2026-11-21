#[cfg(any(target_os = "macos", test))]
pub(crate) mod outlook_columns;
pub mod outlook_entry;
#[cfg(any(target_os = "windows", target_os = "macos", test))]
pub(crate) mod raw_outlook_event;
