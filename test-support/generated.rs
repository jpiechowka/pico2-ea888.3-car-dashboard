//! Failure-only context for bounded generated tests in the separate workspaces.
/// Scope guard printing reproducible case context only during panic unwinding.
/// In panic-abort builds Drop does not run; ordinary success emits no output.
pub struct Case {
    /// Static generated-test family label, without private input payloads.
    name: &'static str,
    /// Deterministic generator seed printed in hexadecimal.
    seed: u64,
    /// Zero-based generated input index.
    index: usize,
}
impl Case {
    /// Return a guard for family `name`, generator `seed` and case `index`; performs no generation or logging yet.
    pub fn new(
        name: &'static str,
        seed: u64,
        index: usize,
    ) -> Self {
        Self { name, seed, index }
    }
}
impl Drop for Case {
    /// Emit the retained label/seed/index to stderr only while unwinding an active test panic.
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "generated case: {} seed={:#x} index={}",
                self.name, self.seed, self.index
            );
        }
    }
}
