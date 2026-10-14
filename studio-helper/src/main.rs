//! Executable shell only. Native actions arrive in later reviewed tasks.
fn main() {
    eprintln!("Native Studio operations are not available in this build.");
    std::process::exit(2);
}
