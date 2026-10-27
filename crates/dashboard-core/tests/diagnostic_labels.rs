//! Compact diagnostics retain the complete existing unit-enum Debug vocabulary.
use dashboard_core::discovery::capture::CaptureError;
use dashboard_core::health::Fault;
use dashboard_core::maintenance::Error;

/// Every capture rejection remains distinguishable without a runtime Debug formatter.
#[test]
fn capture_labels_preserve_diagnostic_names() {
    for value in [
        CaptureError::BufferTooSmall,
        CaptureError::TooLarge,
        CaptureError::NoPeer,
        CaptureError::InvalidState,
        CaptureError::Truncated,
        CaptureError::Magic,
        CaptureError::Version,
        CaptureError::Integrity,
        CaptureError::InvalidData,
        CaptureError::Relationships,
        CaptureError::Trailing,
    ] {
        assert_eq!(value.label(), format!("{value:?}"));
    }
}

/// Every maintenance rejection retains its existing exact diagnostic spelling.
#[test]
fn maintenance_labels_preserve_diagnostic_names() {
    for value in [
        Error::State,
        Error::Generation,
        Error::Pending,
        Error::Owner,
        Error::Clock,
        Error::Timeout,
    ] {
        assert_eq!(value.label(), format!("{value:?}"));
    }
}

/// Every watchdog cause retains its existing exact diagnostic spelling.
#[test]
fn health_labels_preserve_diagnostic_names() {
    for value in [
        Fault::Clock,
        Fault::Supervisor,
        Fault::Acquisition,
        Fault::Rendering,
        Fault::Input,
        Fault::Display,
        Fault::Maintenance,
    ] {
        assert_eq!(value.label(), format!("{value:?}"));
    }
}
