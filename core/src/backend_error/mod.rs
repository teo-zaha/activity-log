//! The one place core's typed failures get their [`BackendErrorKind`]: what the API answers and
//! the desktop commands fail with.

use crate::{
    export::ExportError,
    menu::MenuError,
    model::{BackendError, BackendErrorKind, ConnectionId, RangeError},
    runtime::RecheckRefused,
    settings::{ConfigEditError, ConfigError, EditError, LoadError, NO_LOCATION},
};

#[cfg(test)]
mod tests;

/// A recheck of `id` that started no check: an unknown connection is not found; a host that
/// shut down is quitting, which is no fault of the request.
pub(crate) fn refused_recheck(refused: RecheckRefused, id: &ConnectionId) -> BackendError {
    match refused {
        RecheckRefused::UnknownConnection => {
            BackendError::new(BackendErrorKind::NotFound, format!("No source {id}"))
        }
        RecheckRefused::ShutDown => BackendError::internal(format!(
            "Activity Log is quitting; {id} was not checked again"
        )),
    }
}

impl From<MenuError> for BackendError {
    fn from(err: MenuError) -> Self {
        let kind = match &err {
            MenuError::DuplicateKey(_)
            | MenuError::DuplicateAccelerator(_)
            | MenuError::UnknownItem(_)
            | MenuError::KindChanged(_) => BackendErrorKind::InvalidRequest,
            MenuError::Stale { .. } => BackendErrorKind::Conflict,
            MenuError::Native(_) => BackendErrorKind::Internal,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<RangeError> for BackendError {
    fn from(err: RangeError) -> Self {
        Self::invalid_request(err.to_string())
    }
}

impl From<ExportError> for BackendError {
    fn from(err: ExportError) -> Self {
        let kind = match &err {
            ExportError::UnknownZone(_) | ExportError::Range(_) => BackendErrorKind::InvalidRequest,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<ConfigError> for BackendError {
    fn from(err: ConfigError) -> Self {
        Self::new(BackendErrorKind::BadConfig, err.to_string())
    }
}

impl From<LoadError> for BackendError {
    fn from(err: LoadError) -> Self {
        match err {
            LoadError::Read { .. } => Self::internal(format!("Reading settings failed: {err}")),
            LoadError::Invalid(err) => err.into(),
        }
    }
}

impl From<ConfigEditError> for BackendError {
    fn from(err: ConfigEditError) -> Self {
        let kind = match &err {
            ConfigEditError::UnknownConnection(_) => BackendErrorKind::NotFound,
            ConfigEditError::SingleInstance(_) => BackendErrorKind::AlreadyExists,
            ConfigEditError::WslOnlyForGit
            | ConfigEditError::EmptyLabel
            | ConfigEditError::UnknownSetting { .. } => BackendErrorKind::InvalidRequest,
            ConfigEditError::NotATableList(_) => BackendErrorKind::BadConfig,
        };
        Self::new(kind, err.to_string())
    }
}

impl From<EditError> for BackendError {
    fn from(err: EditError) -> Self {
        let kind = match err {
            EditError::NoLocation => return Self::new(BackendErrorKind::NoLocation, NO_LOCATION),
            EditError::Rejected(err) => return err.into(),
            EditError::Invalid(err) => return err.into(),
            EditError::Internal(err) => {
                return Self::internal(format!("Saving settings failed: {err:#}"));
            }
            EditError::Busy => BackendErrorKind::Busy,
            EditError::Conflict(_) | EditError::FileChanged { .. } => BackendErrorKind::Conflict,
            EditError::Unprotectable { .. } => BackendErrorKind::Unprotectable,
        };
        Self::new(kind, err.to_string())
    }
}
