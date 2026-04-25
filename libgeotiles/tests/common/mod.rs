//! Shared test helpers for `libgeotiles` integration tests.
//!
//! # Fixture resolution order
//!
//! 1. `GEOTILES_TEST_DATA_DIR` — if set, look for `filename` in that directory;
//!    skip download entirely.
//! 2. `GEOTILES_TEST_FETCH` — if set, download from the manifest URL into the
//!    XDG cache dir (`$XDG_CACHE_HOME/geotiles-rs/` or `~/.cache/geotiles-rs/`),
//!    verify SHA-256 when present, then return the cached path.
//! 3. Neither set — return `None`; callers must handle this (tests should be
//!    `#[ignore]`d so plain `cargo test` stays offline).
//!
//! # Usage in integration tests
//!
//! ```rust,ignore
//! mod common;
//!
//! #[test]
//! #[ignore = "requires GEOTILES_TEST_FETCH or GEOTILES_TEST_DATA_DIR"]
//! fn test_with_real_geotiff() {
//!     let path = match common::ensure_fixture("byte_tif") {
//!         Some(p) => p,
//!         None => {
//!             eprintln!("skipping: set GEOTILES_TEST_FETCH or GEOTILES_TEST_DATA_DIR");
//!             return;
//!         }
//!     };
//!     // use path …
//! }
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

// ── Manifest types ────────────────────────────────────────────────────────────

/// A single entry from `fixtures_manifest.toml`.
#[derive(Debug)]
struct Fixture {
    name: String,
    filename: String,
    url: String,
    /// Optional expected hex-encoded SHA-256 digest.
    sha256: Option<String>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Resolve a named fixture to a local file path, downloading if necessary.
///
/// Returns `None` when neither `GEOTILES_TEST_DATA_DIR` nor `GEOTILES_TEST_FETCH`
/// is set, or when the named fixture is not found in the manifest.
///
/// Panics on I/O errors, bad downloads, or SHA-256 mismatches — these indicate
/// a broken test environment and should not be silently ignored.
pub fn ensure_fixture(name: &str) -> Option<PathBuf> {
    let fixtures = load_manifest();
    let entry = fixtures.into_iter().find(|f| f.name == name)?;

    // ── Resolution path 1: local directory override ───────────────────────
    if let Some(dir) = std::env::var_os("GEOTILES_TEST_DATA_DIR") {
        let path = PathBuf::from(dir).join(&entry.filename);
        assert!(
            path.exists(),
            "GEOTILES_TEST_DATA_DIR is set but fixture file does not exist: {}",
            path.display()
        );
        if let Some(ref expected) = entry.sha256 {
            verify_sha256(&path, expected);
        }
        return Some(path);
    }

    // ── Resolution path 2: download + cache ───────────────────────────────
    if std::env::var_os("GEOTILES_TEST_FETCH").is_some() {
        let cache = cache_dir();
        std::fs::create_dir_all(&cache)
            .unwrap_or_else(|e| panic!("cannot create fixture cache dir {}: {e}", cache.display()));
        let dest = cache.join(&entry.filename);

        // Re-use cached file when hash matches; re-download on mismatch.
        if dest.exists() {
            if let Some(ref expected) = entry.sha256 {
                if sha256_of(&dest) == *expected {
                    return Some(dest);
                }
                eprintln!(
                    "cached fixture '{}' hash mismatch — re-downloading",
                    entry.filename
                );
                std::fs::remove_file(&dest).ok();
            } else {
                // No hash to check; trust the cached file.
                return Some(dest);
            }
        }

        download(&entry.url, &dest);

        if let Some(ref expected) = entry.sha256 {
            verify_sha256(&dest, expected);
        }

        return Some(dest);
    }

    // ── Resolution path 3: nothing available ─────────────────────────────
    None
}

/// Return the cache directory for fixture files.
///
/// Prefers `$XDG_CACHE_HOME/geotiles-rs`; falls back to `~/.cache/geotiles-rs`.
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| std::env::temp_dir());
    base.join("geotiles-rs")
}

// ── Manifest loading ──────────────────────────────────────────────────────────

/// Parse `tests/fixtures_manifest.toml` into a list of [`Fixture`] entries.
///
/// The manifest is embedded at compile time via `include_str!` so the helper
/// works regardless of the working directory when tests are run.
fn load_manifest() -> Vec<Fixture> {
    const RAW: &str = include_str!("../fixtures_manifest.toml");
    parse_manifest(RAW)
}

/// Minimal hand-rolled TOML parser for the `[[fixture]]` array.
///
/// We intentionally avoid pulling in a TOML library as a `dev-dependency` here
/// (the manifest format is simple enough to parse line-by-line, and keeping
/// test helpers lean reduces compile times).
fn parse_manifest(src: &str) -> Vec<Fixture> {
    let mut fixtures: Vec<Fixture> = Vec::new();

    // Working state for the current `[[fixture]]` block.
    let mut name: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut url: Option<String> = None;
    let mut sha256: Option<String> = None;
    let mut in_fixture = false;

    for raw_line in src.lines() {
        // Strip inline comments and trailing whitespace.
        let line = match raw_line.find('#') {
            Some(pos) => raw_line[..pos].trim(),
            None => raw_line.trim(),
        };

        if line.is_empty() {
            continue;
        }

        if line == "[[fixture]]" {
            // Commit the previous block (if any).
            if in_fixture {
                if let (Some(n), Some(f), Some(u)) = (name.take(), filename.take(), url.take()) {
                    fixtures.push(Fixture {
                        name: n,
                        filename: f,
                        url: u,
                        sha256: sha256.take(),
                    });
                }
            }
            in_fixture = true;
            name = None;
            filename = None;
            url = None;
            sha256 = None;
            continue;
        }

        if !in_fixture {
            continue;
        }

        // Parse `key = "value"` pairs.
        if let Some((key, val)) = split_kv(line) {
            match key {
                "name" => name = Some(val.to_owned()),
                "filename" => filename = Some(val.to_owned()),
                "url" => url = Some(val.to_owned()),
                "sha256" => sha256 = Some(val.to_owned()),
                // Ignore `description` and any future keys.
                _ => {}
            }
        }
    }

    // Commit the last block.
    if in_fixture {
        if let (Some(n), Some(f), Some(u)) = (name, filename, url) {
            fixtures.push(Fixture {
                name: n,
                filename: f,
                url: u,
                sha256,
            });
        }
    }

    fixtures
}

/// Split `key = "value"` (or multi-line string opening) into `(key, value)`.
///
/// Returns `None` for lines that are not simple key-value pairs.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let (k, rest) = line.split_once('=')?;
    let k = k.trim();
    let v = rest.trim();

    // Handle single-line quoted string: `key = "value"`.
    if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 {
        return Some((k, &v[1..v.len() - 1]));
    }

    // Handle triple-quoted (multi-line) string opening — collapse to first line.
    if v.starts_with("\"\"\"") {
        let inner = v.trim_start_matches('"').trim();
        // Return everything up to the first backslash continuation or end.
        let inner = inner.trim_end_matches('\\').trim();
        return Some((k, inner));
    }

    None
}

// ── Download ──────────────────────────────────────────────────────────────────

/// Download `url` to `dest`, panicking on any error.
///
/// Uses only the standard library (no `reqwest`/`ureq` dependency).
/// Follows up to 5 HTTP redirects.
fn download(url: &str, dest: &Path) {
    eprintln!("downloading fixture: {url}");

    // Simple redirect-following loop using std::net + manual HTTP/1.1.
    // For HTTPS we spawn `curl` as a subprocess — it is available in all
    // supported CI environments (Arch Linux container image) and avoids
    // pulling in a TLS stack as a dev-dependency.
    let status = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--location", // follow redirects
            "--max-redirs",
            "5",
            "--max-time",
            "120", // 2-minute timeout
            "--output",
            dest.to_str().expect("dest path is not valid UTF-8"),
            url,
        ])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn curl: {e}"));

    assert!(
        status.success(),
        "curl failed with exit code {:?} for URL: {url}",
        status.code()
    );

    eprintln!(
        "downloaded {} ({} bytes)",
        dest.display(),
        std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0)
    );
}

// ── SHA-256 ───────────────────────────────────────────────────────────────────

/// Assert that `path` has the expected hex SHA-256 digest, panicking with a
/// descriptive message on mismatch.
fn verify_sha256(path: &Path, expected: &str) {
    let actual = sha256_of(path);
    assert_eq!(
        actual,
        expected,
        "SHA-256 mismatch for fixture {}\n  expected: {expected}\n  actual:   {actual}",
        path.display()
    );
}

// ── SHA-256 via sha256sum subprocess ─────────────────────────────────────────
//
// Uses the `sha256sum` CLI (part of GNU coreutils; present in all supported CI
// images) so we avoid pulling in a `sha2` dev-dependency.

fn sha256_of(path: &Path) -> String {
    let out = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn sha256sum: {e}"));

    assert!(
        out.status.success(),
        "sha256sum failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );

    // Output format: "<hex>  <path>\n"
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("unexpected sha256sum output: {stdout}"))
        .to_owned()
}
