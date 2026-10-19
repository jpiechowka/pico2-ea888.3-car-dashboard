/// Failure to decode a controller's legacy advertising-report batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanErrorKind {
    InvalidSize,
    InvalidValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanFailure {
    pub kind: ScanErrorKind,
    pub revision: u32,
    /// Zero-based position in the controller's batch, not a catalogue index.
    pub index: u8,
    pub total_reports: u8,
    pub at_ms: u64,
}

/// Global counters survive ordinary scan pauses and catalogue clears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanDiagnostics {
    pub total: u32,
    pub invalid_size: u32,
    pub invalid_value: u32,
    pub last: Option<ScanFailure>,
}

impl Default for ScanDiagnostics {
    fn default() -> Self { Self::new() }
}

impl ScanDiagnostics {
    pub const fn new() -> Self {
        Self {
            total: 0,
            invalid_size: 0,
            invalid_value: 0,
            last: None,
        }
    }

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
mod tests {
    use super::*;

    #[test]
    fn errors_retain_their_kind_and_latest_batch_context() {
        let mut diagnostics = ScanDiagnostics::new();
        let first = ScanFailure {
            kind: ScanErrorKind::InvalidSize,
            revision: 7,
            index: 2,
            total_reports: 4,
            at_ms: 100,
        };
        let latest = ScanFailure {
            kind: ScanErrorKind::InvalidValue,
            revision: 9,
            index: 0,
            total_reports: 1,
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
