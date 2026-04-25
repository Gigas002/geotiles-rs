// Smoke-test: calling `init` must not panic under any valid environment state.
//
// Note: `tracing_subscriber::fmt().init()` installs a global subscriber and will
// panic if called a second time in the same process.  We therefore use the
// `try_init` path exposed by `super::try_init` so each test can call it without
// risk of a double-init panic.

#[test]
fn init_does_not_panic() {
    // A second call in the same test binary would be a no-op / return Err, which
    // we intentionally ignore — the important thing is that it never panics.
    let _ = super::try_init();
}
