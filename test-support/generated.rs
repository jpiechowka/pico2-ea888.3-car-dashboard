//! Failure-only context for bounded generated tests in the separate workspaces.
pub struct Case {
    name: &'static str,
    seed: u64,
    index: usize,
}
impl Case {
    pub fn new(
        name: &'static str,
        seed: u64,
        index: usize,
    ) -> Self {
        Self { name, seed, index }
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "generated case: {} seed={:#x} index={}",
                self.name, self.seed, self.index
            );
        }
    }
}
