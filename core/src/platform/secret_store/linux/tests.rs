use std::{io, sync::Arc};

use super::{DbusError, Step, StoreErrorKind, error, zbus};

#[test]
fn a_missing_bus_socket_is_unavailable_only_when_connecting() {
    for (io_kind, connect_kind) in [
        (io::ErrorKind::NotFound, StoreErrorKind::Unavailable),
        (
            io::ErrorKind::ConnectionRefused,
            StoreErrorKind::Unavailable,
        ),
        (io::ErrorKind::PermissionDenied, StoreErrorKind::Failed),
    ] {
        let connection_error = zbus::Error::Connection(
            Arc::new(io_kind.into()),
            Box::new("unix:path=/nonexistent/bus".parse().unwrap()),
        );
        assert_eq!(
            error(
                DbusError::ZBus(connection_error.clone()),
                "Connecting",
                Step::Connect,
            )
            .kind(),
            connect_kind
        );
        assert_eq!(
            error(
                DbusError::ZBus(connection_error),
                "Using the service",
                Step::Use
            )
            .kind(),
            StoreErrorKind::Failed
        );
    }
}
