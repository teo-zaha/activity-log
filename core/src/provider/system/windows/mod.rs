use crate::platform::system_log_access;

use super::SystemSupport;

pub async fn check_system_support() -> SystemSupport {
    match tokio::task::spawn_blocking(|| system_log_access("System"))
        .await
        .map_err(|err| err.to_string())
        .and_then(|result| result.map_err(|err| err.to_string()))
    {
        Ok(()) => SystemSupport::Available,
        Err(err) => SystemSupport::Unavailable(err),
    }
}
