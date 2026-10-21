//! Retained scan-batch framing evidence, independent of catalogue and inspection lifetimes.
/// Failure to decode a controller's legacy advertising-report batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanErrorKind {
    /// Batch/record length was insufficient or inconsistent.
    InvalidSize,
    /// Decoded report contained a value outside the supported controller representation.
    InvalidValue,
}

/// Bounded context for one rejected controller batch record; never a reconstructed packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanFailure {
    /// Framing versus value failure classification.
    pub kind: ScanErrorKind,
    /// Scan request generation producing this error.
    pub revision: u32,
    /// Zero-based position in the controller's batch, not a catalogue index.
    pub index: u8,
    /// Report count declared by the controller batch header.
    pub total_reports: u8,
    /// Concatenated HCI report bytes, excluding the subevent and report count.
    pub report_bytes: u16,
    /// Bytes from the failing record's start through the batch end.
    pub remaining_bytes: u16,
    /// Failed record's Data_Length byte, absent if its header is truncated.
    pub declared_data_len: Option<u8>,
    /// Boot-relative receipt time in milliseconds.
    pub at_ms: u64,
}

/// Global counters survive ordinary scan pauses and catalogue clears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanDiagnostics {
    /// Saturating count of all recorded scan failures.
    pub total: u32,
    /// Saturating count of length/framing failures.
    pub invalid_size: u32,
    /// Saturating count of decoded-value failures.
    pub invalid_value: u32,
    /// Most recently recorded failure, still replaced after counters saturate.
    pub last: Option<ScanFailure>,
}

impl Default for ScanDiagnostics {
    /// Returns zero counters with no retained failure.
    fn default() -> Self { Self::new() }
}

impl ScanDiagnostics {
    /// Returns zero counters with no retained failure; performs no radio operation.
    pub const fn new() -> Self {
        Self {
            total: 0,
            invalid_size: 0,
            invalid_value: 0,
            last: None,
        }
    }

    /// Adds `failure` to `self`'s total/category counts with saturation and replaces
    /// latest context, returning unit. Does not reject older timestamps or generations.
    pub fn record(
        &mut self,
        failure: ScanFailure,
    ) {
        self.total = self.total.saturating_add(1);
        match failure.kind {
            ScanErrorKind::InvalidSize => self.invalid_size = self.invalid_size.saturating_add(1),
            ScanErrorKind::InvalidValue => self.invalid_value = self.invalid_value.saturating_add(1),
        }
        self.last = Some(failure);
    }
}

#[cfg(test)]
/// Counter/provenance regressions independent of the radio transport.
mod tests {
    use super::*;

    #[test]
    /// Returns unit after checking separate category counts and latest context replacement;
    /// panics if a later failure loses its generation, framing or timestamp evidence.
    fn errors_retain_their_kind_and_latest_batch_context() {
        let mut diagnostics = ScanDiagnostics::new();
        let first = ScanFailure {
            kind: ScanErrorKind::InvalidSize,
            revision: 7,
            index: 2,
            total_reports: 4,
            report_bytes: 100,
            remaining_bytes: 10,
            declared_data_len: Some(31),
            at_ms: 100,
        };
        let latest = ScanFailure {
            kind: ScanErrorKind::InvalidValue,
            revision: 9,
            index: 0,
            total_reports: 1,
            report_bytes: 8,
            remaining_bytes: 8,
            declared_data_len: None,
            at_ms: 240,
        };
        diagnostics.record(first);
        assert_eq!(diagnostics.last, Some(first));
        diagnostics.record(latest);
        assert_eq!(
            (diagnostics.total, diagnostics.invalid_size, diagnostics.invalid_value),
            (2, 1, 1)
        );
        assert_eq!(diagnostics.last, Some(latest));
    }

    #[test]
    /// Returns unit after confirming saturated counters still replace latest evidence
    /// for both categories; panics on count wrap or frozen diagnostic context.
    fn saturated_counts_still_replace_the_latest_failure() {
        let mut diagnostics = ScanDiagnostics {
            total: u32::MAX,
            invalid_size: u32::MAX,
            invalid_value: u32::MAX,
            last: None,
        };
        for kind in [ScanErrorKind::InvalidSize, ScanErrorKind::InvalidValue] {
            let failure = ScanFailure {
                kind,
                revision: 12,
                index: 1,
                total_reports: 3,
                report_bytes: 60,
                remaining_bytes: 20,
                declared_data_len: Some(15),
                at_ms: 400,
            };
            diagnostics.record(failure);
            assert_eq!(
                (diagnostics.total, diagnostics.invalid_size, diagnostics.invalid_value),
                (u32::MAX, u32::MAX, u32::MAX)
            );
            assert_eq!(diagnostics.last, Some(failure));
        }
    }
}
