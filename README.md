# geotiles-rs

A Rust library for generating map tiles from GeoTIFF sources.

> **Status:** early development — see [`docs/GEOTILES_RUST_MIGRATION_PLAN.md`](docs/GEOTILES_RUST_MIGRATION_PLAN.md) for the full roadmap.

---

## Table of contents

- [System dependencies](#system-dependencies)
- [Feature flags](#feature-flags)
- [Building](#building)
- [Running tests](#running-tests)
- [Running benchmarks](#running-benchmarks)
- [Benchmark results](#benchmark-results)

---

## System dependencies

| Dependency     | Required by    | Notes                                                                                                     |
| -------------- | -------------- | --------------------------------------------------------------------------------------------------------- |
| GDAL (≥ 3.x)   | always         | `libgdal-dev` on Debian/Ubuntu, `gdal` on Arch/Homebrew                                                   |
| libaom / rav1e | `avif` feature | pulled in transitively by the `image/avif` codec (pure Rust via `ravif`) — no extra system package needed |

---

## Feature flags

All format features are **opt-in**. `png` and the coordinate system features (`geographic`, `mercator`) are on by default.

| Feature      | What it enables                                 | Default |
| ------------ | ----------------------------------------------- | ------- |
| `png`        | PNG tile encoding                               | ✅      |
| `jpeg`       | JPEG tile encoding                              | ❌      |
| `webp`       | WebP tile encoding (lossless only for now)      | ❌      |
| `avif`       | AVIF tile encoding via `ravif` (pure Rust)      | ❌      |
| `jxl`        | JPEG XL encoding via `jxl-encoder` (pure Rust)  | ❌      |
| `geographic` | Geographic (EPSG:4326) tile coordinate system   | ✅      |
| `mercator`   | Web Mercator (EPSG:3857) tile coordinate system | ✅      |
| `gpu`        | Reserved for future GPU tile pipeline (Phase 7) | ❌      |

---

## Building

```sh
# Default features (PNG + both coordinate systems):
cargo build -p libgeotiles

# Specific output formats:
cargo build -p libgeotiles --features jpeg,webp

# All stable formats:
cargo build -p libgeotiles --features png,jpeg,webp,avif

# Everything (all formats, pure Rust):
cargo build -p libgeotiles --all-features
```

---

## Running tests

```sh
# Default features only:
cargo test -p libgeotiles

# All features (pure Rust, no system dependencies beyond GDAL):
cargo test -p libgeotiles --all-features

# Entire workspace:
cargo test --all-features

# A specific test by name:
cargo test -p libgeotiles --all-features -- encode::tests::jxl_lossless_option_accepted
```

Test output is suppressed by default. Pass `-- --nocapture` to see `tracing` log output from a test:

```sh
RUST_LOG=debug cargo test -p libgeotiles --all-features -- --nocapture
```

---

## Running benchmarks

Benchmarks use [Criterion](https://crates.io/crates/criterion) and live in `libgeotiles/benches/`.  
They are **not** executed by `cargo test` — they must be invoked explicitly.

### Quick start

```sh
# Run all benchmarks with default features (PNG only):
cargo bench -p libgeotiles --bench encode

# Run with all format encoders enabled:
cargo bench -p libgeotiles --bench encode --features png,jpeg,webp,avif,jxl
```

### Targeting a specific benchmark group

Pass a filter substring after `--`; Criterion matches it against group and benchmark names:

```sh
# Only PNG benchmarks:
cargo bench -p libgeotiles --bench encode --features png -- encode_png

# Only the AVIF q60/speed=6 case:
cargo bench -p libgeotiles --bench encode --features avif -- encode_avif/rgb256_q60/6
```

### Viewing HTML reports

Criterion writes an HTML report after every run:

```
target/criterion/encode_png/rgba256/default/report/index.html
```

Open it in a browser:

```sh
xdg-open target/criterion/report/index.html   # Linux
open target/criterion/report/index.html        # macOS
```

### Comparing two runs (before / after a change)

Criterion automatically compares the current run against the last saved baseline and prints a regression or improvement notice:

```
encode_png/rgba256/default
                        time:   [1.1234 ms 1.1301 ms 1.1372 ms]
                 change:   [-3.1234% -2.8901% -2.6012%] (p = 0.00 < 0.05)
                        Performance has improved.
```

To **save a named baseline** before making changes:

```sh
cargo bench -p libgeotiles --bench encode --features png,jpeg,webp -- --save-baseline main
```

Then, after your change:

```sh
cargo bench -p libgeotiles --bench encode --features png,jpeg,webp -- --baseline main
```

### Benchmark descriptions

| Group         | Feature flag | What is measured                                                                                                    |
| ------------- | ------------ | ------------------------------------------------------------------------------------------------------------------- |
| `encode_png`  | `png`        | PNG encoder throughput at three compression levels (`default`, `fast`, `best`) for a synthetic 256×256 RGBA tile    |
| `encode_jpeg` | `jpeg`       | JPEG encoder throughput at quality settings 50 / 75 / 85 / 95, plus an RGBA → RGB alpha-strip path                  |
| `encode_webp` | `webp`       | Lossless WebP throughput for RGB and RGBA inputs                                                                    |
| `encode_avif` | `avif`       | AVIF encoder throughput at encoder speed 4 / 6 / 8 (quality 60); sample count is reduced to 20 because AVIF is slow |
| `encode_jxl`  | `jxl`        | JPEG XL throughput at distance 1.0 effort 7, distance 2.0 effort 5, and lossless (distance 0.0) effort 7            |

> **Note on AVIF and JXL bench times:** these encoders are significantly slower than PNG/JPEG/WebP. Set aside a few minutes for a full run with all features. Pass `--sample-size 10` to Criterion for a quicker (less statistically accurate) estimate during development.

```sh
cargo bench -p libgeotiles --bench encode --features avif,jxl -- --sample-size 10
```

---

## Benchmark results

Baseline snapshots are recorded in [`docs/benchmarks/`](docs/benchmarks/) as Markdown/CSV files whenever a path is first stabilised.  
Do **not** rely solely on Criterion's local HTML report for tracking regressions — commit a snapshot after any significant change to the encode or resample hot path.

CI does **not** run benchmarks on every pull request. A separate optional workflow (manual trigger or `[bench]` commit tag) is used for reproducible performance comparisons on a pinned runner.
