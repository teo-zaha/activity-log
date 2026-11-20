use std::path::PathBuf;

use crate::{
    export::ExportError,
    model::{BackendError, BackendErrorKind, ConnectionId, RangeError, Source},
    settings::{ConfigEditError, ConfigError, Conflict, ConflictKind, EditError, LoadError},
};

fn kind(error: impl Into<BackendError>) -> BackendErrorKind {
    error.into().kind
}

fn connection() -> ConnectionId {
    ConnectionId::new(Source::Gitlab, "work")
}

fn config_error() -> ConfigError {
    ConfigError {
        path: PathBuf::from("config.toml"),
        line: 1,
        column: 2,
        message: "expected `=`".to_string(),
    }
}

#[test]
fn a_bad_range_or_zone_is_an_invalid_request() {
    for range in [
        RangeError::NotRfc3339,
        RangeError::Empty,
        RangeError::TooLong,
    ] {
        assert_eq!(kind(range), BackendErrorKind::InvalidRequest);
        assert_eq!(
            kind(ExportError::Range(range)),
            BackendErrorKind::InvalidRequest
        );
    }
    assert_eq!(
        kind(ExportError::UnknownZone("Mars/Base".to_string())),
        BackendErrorKind::InvalidRequest
    );
}

#[test]
fn each_refused_config_edit_has_its_kind() {
    let cases = [
        (
            ConfigEditError::UnknownConnection(connection()),
            BackendErrorKind::NotFound,
        ),
        (
            ConfigEditError::SingleInstance(Source::Outlook),
            BackendErrorKind::AlreadyExists,
        ),
        (
            ConfigEditError::WslOnlyForGit,
            BackendErrorKind::InvalidRequest,
        ),
        (
            ConfigEditError::EmptyLabel,
            BackendErrorKind::InvalidRequest,
        ),
        (
            ConfigEditError::UnknownSetting {
                source: Source::Jira,
                key: "GIT_DIRS",
            },
            BackendErrorKind::InvalidRequest,
        ),
        (
            ConfigEditError::UnknownProvider(Source::Jira),
            BackendErrorKind::InvalidRequest,
        ),
        (
            ConfigEditError::NotATableList(Source::Gitlab),
            BackendErrorKind::BadConfig,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(kind(error.clone()), expected, "{error:?}");
        assert_eq!(kind(EditError::Rejected(error)), expected);
    }
}

#[test]
fn each_failed_edit_has_its_kind() {
    let conflict = Conflict {
        connection: connection(),
        label: None,
        kind: ConflictKind::Changed,
        fields: Vec::new(),
    };
    let cases = [
        (EditError::NoLocation, BackendErrorKind::NoLocation),
        (EditError::Busy, BackendErrorKind::Busy),
        (EditError::Conflict(conflict), BackendErrorKind::Conflict),
        (
            EditError::FileChanged {
                name: "config.toml".to_string(),
            },
            BackendErrorKind::Conflict,
        ),
        (
            EditError::Invalid(config_error()),
            BackendErrorKind::BadConfig,
        ),
        (
            EditError::Unprotectable {
                location: PathBuf::from("/Volumes/USB"),
            },
            BackendErrorKind::Unprotectable,
        ),
        (
            EditError::Internal(anyhow::anyhow!("disk full")),
            BackendErrorKind::Internal,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(kind(error), expected);
    }
}

#[test]
fn a_config_that_does_not_read_is_a_bad_config_and_a_failed_read_internal() {
    assert_eq!(kind(config_error()), BackendErrorKind::BadConfig);
    assert_eq!(
        kind(LoadError::Invalid(config_error())),
        BackendErrorKind::BadConfig
    );
    let unreadable = LoadError::Read {
        path: PathBuf::from("config.toml"),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    };
    assert_eq!(kind(unreadable), BackendErrorKind::Internal);
}

#[test]
fn the_wire_form_keeps_the_error_field() {
    let body = serde_json::to_value(BackendError::new(BackendErrorKind::Busy, "busy")).unwrap();
    assert_eq!(body, serde_json::json!({ "kind": "busy", "error": "busy" }));
}
