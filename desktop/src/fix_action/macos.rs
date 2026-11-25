use std::{process::Stdio, time::Duration};

use activity_log_core::{
    model::{BackendError, LocalApp, LocalFixTarget, SystemSettingsPane},
    provider::outlook::OUTLOOK_BUNDLE_ID,
};

const AUTOMATION: &str =
    "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation";
const FILES_AND_FOLDERS: &str = "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_FilesAndFolders";

pub(super) fn command(target: LocalFixTarget) -> std::process::Command {
    let mut command = std::process::Command::new("/usr/bin/open");
    match target {
        LocalFixTarget::SystemSettings { pane } => {
            command.arg(match pane {
                SystemSettingsPane::Automation => AUTOMATION,
                SystemSettingsPane::FilesAndFolders => FILES_AND_FOLDERS,
            });
        }
        LocalFixTarget::OpenApp {
            app: LocalApp::Outlook,
        } => {
            command.args(["-b", OUTLOOK_BUNDLE_ID]);
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

pub(super) async fn open(target: LocalFixTarget) -> Result<(), BackendError> {
    let mut command = tokio::process::Command::from(command(target));
    command.kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(10), command.status())
        .await
        .map_err(|_| BackendError::internal("macOS did not answer the open request in time"))?
        .map_err(|err| {
            BackendError::internal(format!("Could not ask macOS to open the target: {err}"))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::internal(format!(
            "macOS could not open the recovery target ({status})"
        )))
    }
}
