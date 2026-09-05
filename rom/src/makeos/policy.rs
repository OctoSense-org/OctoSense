/// Optional background work is enabled explicitly for this invocation.
pub fn requested(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}
