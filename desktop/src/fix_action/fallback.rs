use activity_log_core::model::{BackendError, LocalFixTarget};

pub(super) async fn open(_target: LocalFixTarget) -> Result<(), BackendError> {
    Err(BackendError::invalid_request(
        "This recovery action is only available on macOS",
    ))
}
