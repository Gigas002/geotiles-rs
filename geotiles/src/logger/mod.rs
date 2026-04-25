//! Logging initialisation.
//!
//! Call [`init`] once at the very start of `main` before any other code runs.
//! It wires up [`tracing_subscriber`] with an [`EnvFilter`] that reads
//! `RUST_LOG`; when the variable is absent or invalid it falls back to `"info"`.

use tracing_subscriber::EnvFilter;

/// Initialise the global tracing subscriber.
///
/// Safe to call only once per process.  Panics if a global subscriber has
/// already been set (mirrors the behaviour of
/// [`tracing_subscriber::fmt::init`]).
pub fn init() {
    try_init().expect("failed to set global tracing subscriber");
}

/// Attempt to initialise the global tracing subscriber without panicking.
///
/// Returns `Ok(())` on success and `Err` if a subscriber is already installed.
/// Prefer this in tests where multiple test binaries may share a process.
pub fn try_init() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init()
}

#[cfg(test)]
mod tests;
