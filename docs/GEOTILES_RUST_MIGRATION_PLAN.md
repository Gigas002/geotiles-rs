# GTiff2Tiles → Rust (`geotiles-rs`) migration plan

This document is both a **human roadmap** and an **agent playbook**: steps are sized for focused implementation sessions, end in a **verified** state (`cargo build`, `cargo fmt`, `cargo clippy`), and state **how to verify**. It follows the structure and discipline of example plans ([tofi-rs `RUST_MIGRATION_PLAN`](https://raw.githubusercontent.com/Gigas002/tofi-rs/refs/heads/v0/docs/RUST_MIGRATION_PLAN.md), [`POST_MIGRATION_PLAN`](https://raw.githubusercontent.com/Gigas002/tofi-rs/refs/heads/v0/docs/POST_MIGRATION_PLAN.md), [imgvwr `IMV_RS_PLAN`](https://raw.githubusercontent.com/Gigas002/imgvwr/19f5e82b6a5cc7b23e2bf25e03ca448b1d8fb109/docs/IMV_RS_PLAN.md)).

**Primary scope:** **`libgeotiles`** — the library, its API, encoders, GDAL/GPU pipeline, tests, and repo tooling (**§2.1**). **CLI binary (`geotiles`), command-line flags, and application config file format are explicitly post–first-release** (see **§1.6** and **§5**).

**Reference product:** [Gigas002/GTiff2Tiles](https://github.com/Gigas002/GTiff2Tiles) — C# library analogous to `gdal2tiles.py` / MapTiler: GeoTIFF → web map tiles (zoom levels, slippy-map layout, CRS handling). The **C# codebase is behavioral reference only**, not an API or architecture spec.

---

## 1. Goals and constraints

### 1.1 Goals

- **Same problem domain** as GTiff2Tiles **Core**: read GeoTIFF (and similar GDAL rasters), optionally reproject, compute **Web Mercator** or **WGS84 geographic** tile grids, **crop/resample** per tile, **encode** tiles, write to `{z}/{x}/{y}` layout with optional **TMS vs XYZ** indexing.
- **Optional tile output formats** (see **§1.5**): **PNG** and **JPEG** as the baseline set; **WebP**, **AVIF**, and **JPEG XL** as **opt-in** Cargo features selected via **library API** (`TileFormat`, build flags); heavy or native-backed codecs stay **optional**. A future CLI will map user input to these types — **not** part of the first release.
- **First release focus:** **`libgeotiles` only** — public API for jobs (`TileJob` / equivalent), pipeline, encoders, optional GPU path, tests, docs in-repo, CI (**§2.1**). **No** shipped CLI binary, **no** committed application-level config schema in v1.
- **Clean-room design:** implement **equivalent functionality** in the **simplest, fastest** way that fits Rust + GDAL. **Do not** mirror C# class hierarchy, exception types, or method signatures.
- **Rust edition:** `2024` in `[workspace.package]` and member crates (align with current ecosystem practice).
- **Repository layout for docs and CI:** follow [**imgvwr**](https://github.com/Gigas002/imgvwr) **with minimal changes** — see **§2.1** (workflows, Dependabot, `deny.toml`, `.typos.toml`, `docs/` conventions).
- **Dependency policy:** prefer crates with **recent releases or maintenance** (roughly **within one year** at dependency lock time). **Reject** abandoned crates; re-evaluate when bumping `Cargo.lock`.
- **Testing:** aim for **broad automated coverage**: pure logic (**unit**), GDAL-backed **integration** tests, and **end-to-end** runs on **real** GeoTIFFs. Large rasters **must not** live in git (and **Git LFS is not used**). Follow **§6** for how to obtain, cache, and optionally skip heavy assets.
- **Resampling / tile “crop” path:** the **intended end-state** is an **optional GPU pipeline** (**wgpu**) for **per-tile crop + scale**. **Shipping defaults:** **`default` features = CPU-only**. **GPU is opt-in** via Cargo feature(s) (e.g. `gpu` / `gpu-vulkan` / `gpu-gles`) and a **library-level** choice on `TileJob` / builder (e.g. `ResampleBackend::Cpu` vs `Gpu`). A **future** CLI may expose `--backend` — **out of scope for first library release** (§1.6).

### 1.2 Non-goals (explicitly out of scope for this document)

- **Standalone documentation product** (public **GitHub Pages** site, **Wiki**, packaged **man pages**) as a migration deliverable — **not** required; **in-repo** docs follow **§2.1**.
- **Inventing CI / repo tooling layout from scratch** — **avoid**. **GitHub Actions**, **Dependabot**, **`deny.toml`**, **`.typos.toml`**, and related files should be **copied from [imgvwr](https://github.com/Gigas002/imgvwr)** and **adapted minimally** (§2.1). **Docker**, **NuGet**, **codecov** flags: only if imgvwr already has an equivalent pattern worth mirroring; otherwise skip.
- **Avalonia / GUI** or any desktop UI.
- **Line-for-line** port of **C#** tests. **First release:** **no** CLI crate, **no** user-facing config file — those are **post–first-release** (§1.6, §5). **Rust** tests for the library should still be **comprehensive** (see **§6**), including real-world GeoTIFFs via **out-of-repo** assets.
- **Pixel-identical** output vs C# or `gdal2tiles.py` for every edge case — document intentional differences if any.

### 1.3 What “parity” means here

| Area | GTiff2Tiles Core (reference) | Target in Rust |
|------|------------------------------|----------------|
| GDAL | `GdalWarp`, `GDALInfo`, geo transform, projection strings | `gdal` crate: open dataset, warp options, read windows, CRS metadata |
| Fast image / tiles | **NetVips** (`Image`, tile cache, parallel crops) | **Baseline (default):** GDAL window read + **`fast_image_resize`** (or GDAL resampling) + **encode** (see §1.5). **Target (optional `gpu`):** **`wgpu`** crop + scale → **readback** → **CPU encode** (same format set as CPU path). |
| Coordinates | `GeodeticCoordinate`, `MercatorCoordinate`, `Number` (x,y,z), TMS flag | Small **Rust types** + **pure functions** (see §3) |
| Orchestration | `TileGenerator`, `Raster`, `RasterTile` | **`libgeotiles::pipeline`** (names illustrative): one clear **tile loop** + **parallelism** (`rayon` or controlled `std::thread`); **backend enum** `Cpu` / `Gpu` behind features |

### 1.4 CPU vs GPU (policy)

| | **CPU (default)** | **GPU (optional, migration target)** |
|--|-------------------|--------------------------------------|
| **Cargo** | In `default` features | Separate feature(s), e.g. `gpu`, `gpu-vulkan`, `gpu-gles`; future **`geotiles`** crate **forwards** the same names (**§1.6**) |
| **Runtime** | Always available | Only if built with GPU features **and** `TileJob` requests GPU; **future** CLI may add `--backend gpu` (**§5**) |
| **Work split** | GDAL → buffer → resize → encode | GDAL → **staging buffer** → **GPU** crop/scale → readback → encode |
| **Failure** | N/A | If GPU init fails, **fall back to CPU** with `tracing::warn!` (or return error — pick one policy and document it) |

Design **`TileJob` / tile step APIs** so the **same** `(z,x,y)` math and **output bytes** contract works for both backends; only the **resample implementation** swaps.

### 1.6 First release vs post-release (CLI and config)

| Milestone | In scope | Out of scope (defer) |
|-----------|----------|----------------------|
| **First release (`libgeotiles` v0.x / 1.0 library)** | Crate **`libgeotiles`**, stable-enough API for tiling jobs, encoders, CPU + optional GPU pipeline, tests, **§7.0** gates on **`-p libgeotiles`**, CI (**§2.1**) targeting the library | **`geotiles`** binary, **`clap`**, argv parsing, **`tracing-subscriber` wiring in a `main`**, **application config file** (TOML/YAML/etc.), env-file conventions |
| **Post–first-release phases** | Add **`geotiles`** workspace member (or separate step), **CLI design from scratch**, **config format** (likely **TOML** — decide when implementing), loading order (defaults → file → env → CLI), shell completions if desired | — |

**Rule:** Do **not** block the library release on CLI or config decisions. **`TileJob`** and related types should be **CLI-agnostic** so a later binary only **constructs** them from parsed args + config.

### 1.5 Output formats (optional; Cargo features + library API)

Support **multiple** container/codec choices; **not** all need to be in `default` features.

| Format | Extension(s) | Typical role | Cargo / notes |
|--------|----------------|----------------|---------------|
| **PNG** | `.png` | Lossless, universal; **default** output | `image` / `png` feature |
| **JPEG** | `.jpg`, `.jpeg` | Photos, smaller than PNG; lossy | `image` / `jpeg` feature |
| **WebP** | `.webp` | Lossy or lossless; good for web maps | `image` / `webp` feature |
| **AVIF** | `.avif` | Modern lossy/lossless; smaller at cost of CPU | `image` **and/or** dedicated encoder crate — **verify** at implementation time that chosen stack is **maintained**; may pull **system** `libavif` or use a **pure-Rust** path (e.g. `ravif` + `dav1d`) — document packager deps |
| **JPEG XL** | `.jxl` | High efficiency; growing viewer support | Often **`jxl-oxide`** (encode) or **`jxl`** — **not** always via `image`; gate behind feature `jxl` |

**Rules**

- **`default` library features:** include at least **PNG** (and optionally **JPEG** if you want “one lossy” out of the box — pick one policy).
- **Library:** `TileFormat` / encoder choice must respect **compiled-in** features; return a clear **error** if a format was requested but the feature is off.
- **Alpha / nodata:** PNG/WebP/AVIF/JXL can carry alpha; JPEG cannot — document **flatten** or **drop alpha** behavior for `.jpg`.
- **Quality:** store optional **quality** in `TileJob` (or parallel field) for lossy formats — a **future** CLI may map `--quality`; not required for first release beyond API support if you want it.

**Deferred to CLI phase:** `--format`, `--quality`, `--output-format` as **user-facing** flags; config keys for defaults.

---

## 2. Repository layout (target)

```text
geotiles-rs/
  Cargo.toml                    # [workspace] members = ["libgeotiles"] initially; add "geotiles" when CLI work starts (§1.6)
  Cargo.lock                    # committed (application workspace)
  libgeotiles/
    Cargo.toml                  # package name = "libgeotiles"
    src/
      lib.rs                    # exports; minimal logic
      error.rs                  # thiserror-based public errors
      crs/                      # CRS detection, EPSG:4326 / EPSG:3857 helpers (thin wrapper over GDAL)
      coords/                   # tile indices, bbox ↔ pixels, TMS/XYZ flip
      gdal_io/                  # open dataset, warp to work CRS, geo transform, raster bands
      tile/                     # single-tile read window, resample, encode bytes
      tile/gpu.rs               # optional: wgpu context, pipelines, readback (behind `feature = "gpu"`)
      pipeline/                 # zoom range, tile enumeration, progress model; dispatches Cpu vs Gpu
      output/                   # directory writer, path pattern `{z}/{x}/{y}.ext`
      encode/                   # RGBA buffer → bytes: dispatch png / jpeg / webp / avif / jxl by `TileFormat`
    tests/
      fixtures_manifest.toml    # optional: stable URLs + SHA-256 for heavy GeoTIFFs (see §6); not the rasters themselves
    examples/                   # optional: small binaries that build TileJob in code (dogfood before CLI — §5)
  # geotiles/                   # POST first library release — see §5
```

**Naming (first release)**

| Role | Cargo `package.name` | Rust crate id |
|------|----------------------|---------------|
| Library | `libgeotiles` | `libgeotiles` |

**Naming (post-release)**

| Role | Cargo `package.name` | Installed binary |
|------|----------------------|------------------|
| CLI | `geotiles` | `geotiles` |

### 2.1 Repository organization — **imgvwr** as canonical template (keep structure **mostly unchanged**)

**Reference repo:** [**Gigas002/imgvwr**](https://github.com/Gigas002/imgvwr) (use default branch or the branch you treat as current for Rust workspace work).

**Copy and adapt with minimal edits** so `geotiles-rs` stays organized like imgvwr:

| Area | Action |
|------|--------|
| **`.github/workflows/`** | Mirror workflow **names**, **matrix style**, **`dtolnay/rust-toolchain`**, **`Swatinem/rust-cache`**, job split (**build** / **fmt-clippy** / **test** / **typos** / **deny** / **deploy** if present). **Until the `geotiles` binary exists**, use **`-p libgeotiles`** (or `--workspace` with a single member) in all **`cargo`** invocations; when adding **`geotiles`**, extend commands to match **imgvwr**’s two-crate pattern (`libimgvwr` → `libgeotiles`, `imgvwr` → `geotiles`). |
| **`.github/dependabot.yml`** | Copy structure; set `directory: "/"` for the workspace root. |
| **`deny.toml`** | Copy **license policy** and structure; adjust crate names / exceptions only if `cargo deny` requires it for GDAL-related SPDX. |
| **`.typos.toml`** | Copy; **extend** `extend-exclude` for GeoTIFF paths, `target/`, cache dirs, and any GDAL-specific false positives as they appear. |
| **Root `Cargo.toml`** | Align **`[workspace.package]`** patterns (edition, license metadata, repository URL) with imgvwr style — point `repository` / `homepage` to **this** repo. |
| **`docs/`** | Keep this migration plan (and any small companion docs) in the same **spirit** as imgvwr’s `docs/` (plan + revision history); **do not** require a separate published site. |

**System packages in CI images:** **Replace** imgvwr’s Wayland / xkb / (optional) libavif with what **this** project needs — at minimum **`libgdal`** / **`gdal`** via distro packages (`libgdal-dev`, `pkg-config`, build-essential). For **`--all-features`** GPU jobs, keep imgvwr’s **Mesa / Vulkan (lavapipe)** pattern if you mirror the **gpu-vulkan** matrix entry. Document replacements in **workflow comments** so the next maintainer sees the diff vs imgvwr.

**Rule:** When adding or changing automation, **open imgvwr side-by-side** and preserve **file layout and naming** unless there is a **project-specific** reason to diverge.

---

## 3. Dependencies (candidates — pin **current latest** at implementation time)

**Policy:** use **two-component** version requirements in `Cargo.toml` (e.g. `0.19`) where practical; exact versions live in **`Cargo.lock`**. Before each release, run `cargo update` and **confirm** each crate still shows activity within ~12 months (crates.io / GitHub). **Do not** depend on unmaintained crates.

| Crate | Role | Notes |
|-------|------|--------|
| [**gdal**](https://crates.io/crates/gdal) | GDAL Dataset, warping, `read_raster`, geotransform, SRS | System **libgdal** required; primary geospatial engine |
| [**thiserror**](https://crates.io/crates/thiserror) | `Error` enums in `libgeotiles` | |
| [**image**](https://crates.io/crates/image) | Encode **PNG**, **JPEG**, **WebP** via selective features | `default-features = false`; enable `png`, `jpeg`, `webp` as needed; **AVIF** via `image` only if you accept its **native** / feature story at lock time |
| **AVIF encoder** (TBD at implementation) | **`.avif`** tiles | e.g. **`ravif`**, or **`image`** with `avif` + system libs — pick **one** maintained path; re-check §3 health policy |
| **JPEG XL encoder** (TBD at implementation) | **`.jxl`** tiles | e.g. **`jxl-oxide`** encode API or **`jxl`** — often **outside** `image`; separate feature `jxl` |
| [**fast_image_resize**](https://crates.io/crates/fast_image_resize) | SIMD-friendly resize to tile size | Alternative: GDAL overview/warp only — pick one path to avoid double work |
| [**rayon**](https://crates.io/crates/rayon) | Parallel tile generation | Optional `features` gate if you want single-threaded builds |
| [**tracing**](https://crates.io/crates/tracing) | Structured logs in library | **`tracing-subscriber`** only when a **`main`** exists (CLI phase) or **dev** tests |
| [**clap**](https://crates.io/crates/clap) | CLI | **Post–first-release** — **`geotiles`** binary only (§1.6) |
| [**memmap2**](https://crates.io/crates/memmap2) | Optional: mmap large reads | Only if profiling shows benefit |
| [**wgpu**](https://crates.io/crates/wgpu) | **Optional (`gpu` feature):** crop + resize on GPU | `default-features = false`; enable `wgsl` + one backend (`vulkan` and/or `gles`) |
| [**pollster**](https://crates.io/crates/pollster) | **Optional:** block on async `wgpu` init / submit without a full async runtime | Same pattern as imgvwr GPU phases |

**Optional / later**

| Crate | Role |
|-------|------|
| [**geo-types**](https://crates.io/crates/geo-types) | `Rect`, `Coord` — only if you want interop; plain `f64` pairs may suffice |
| [**proj**](https://crates.io/crates/proj) | PROJ bindings — **avoid duplicating GDAL** unless you need transforms without GDAL |

**Deferred / usually not needed**

- **Full libvips bindings** — duplicates GDAL; only revisit if GDAL+GPU path is insufficient **and** profiling points at I/O or decode.

### 3.1 Cargo features (illustrative)

**`libgeotiles`**

```toml
[features]
default = ["png"]               # example: lossless default; add "jpeg" if desired
png = ["image/png"]
jpeg = ["image/jpeg"]
webp = ["image/webp"]
avif = [/* ravif or image/avif — exact deps TBD */]
jxl = [/* jxl-oxide or jxl — exact deps TBD */]
gpu = ["dep:wgpu", "dep:pollster", ...]   # implement as gpu-vulkan / gpu-gles if you split backends
```

**`geotiles`** (when the crate exists) — mirror **`libgeotiles`** format features **and** GPU features (same names).

**Rules**

- Workspace **default** features must **not** enable `gpu` / `wgpu` — only explicit `--features gpu` (or `all-features`) pulls it in.
- **`--all-features`** must compile every format + GPU; **`--no-default-features`** defines a **minimal** matrix (e.g. no encoders unless `--features png` is passed) — document the intended combo.

**System dependencies (packagers):** GDAL (`libgdal`), C compiler for `gdal-sys` if using bindgen, standard build tools. **If AVIF** uses a native path: **`libavif`** / **dav1d** as required by the chosen crate — list in README when you document packaging.

---

## 4. Functional decomposition (library)

Implement **features**, not **C# types**:

1. **Open source** — path in → `Dataset` (read-only), band count, dtype, nodata.
2. **Working CRS** — normalize to **EPSG:3857** (typical web maps) or **EPSG:4326** via GDAL warp to a **temporary** or **in-memory** dataset (strategy: temp GeoTIFF vs `VRT` — choose simplest robust option).
3. **Extent** — from geotransform + size, in the **working CRS**; helpers for **tile index range** for given `z`, tile size (256 default), **TMS** y-order flag.
4. **Per-tile pipeline** — for `(z, x, y)`: compute **source pixel window** (and subpixel bounds), read buffer, **resample** to `tile_size × tile_size` (**CPU** or **GPU** per §1.4), **encode** to bytes on CPU using **`TileFormat`** (§1.5): **png**, **jpeg**, **webp**, **avif**, **jxl** as enabled by features.
5. **Output** — write files under `output/{z}/{x}/{y}.{ext}` matching the selected format; optional **metadata** file (e.g. simple `bounds` JSON) — **minimal**, only if needed for web viewers; skip gdal2tiles’ full XML suite unless required.

**Reuse ideas from current repo:** `main.rs` already sketches **resolution**, **pixel/tile numbers**, and **`get_areas`**-style read/write regions — **refactor into `libgeotiles::coords`** and validate against GDAL geotransform math (do not trust duplicated formulas without tests against GDAL).

---

## 5. CLI and application config — **post–first-release** (not part of initial library milestone)

**Status:** **Deferred** until **`libgeotiles`** reaches the **first release** criteria (**§9.1**). Design the **`geotiles`** binary and **user-facing config** in a **separate** planning pass so the **library API** stays stable and **CLI-agnostic**.

**Rough direction** (non-binding — revisit when starting this phase):

- **`geotiles`** crate: **`clap`** for argv, thin **`main`**, **`tracing-subscriber`** for logs.
- **Config file:** format **TBD** (often **TOML**); resolution order **TBD** (e.g. XDG config dir + `--config` override). Must map cleanly onto **`TileJob`** / library types — **no** business logic in the binary beyond parsing and wiring.
- Flags (illustrative only): input GeoTIFF/VRT, output directory, zoom range, tile size, TMS/XYZ, **`--format`**, **`--quality`**, **`--threads`**, **`--backend cpu|gpu`** when GPU feature is on, etc.

**First-release substitute for dogfooding:** **`examples/`** binaries or **integration tests** that build **`TileJob`** in code and call the public API — no separate config file required.

---

## 6. Testing strategy and GeoTIFF fixtures (no Git LFS)

Large GeoTIFFs **cannot** be committed to the repo. **Git LFS is not an option.** Use a **layered** approach so `cargo test` is **fast and offline-friendly by default**, while still allowing **full** validation when assets and network are available.

### 6.1 What always lives in the repo (small)

- **Unit tests** for **pure** code: `coords`, tile math, path patterns — **no** GDAL, **no** large files. (**CLI parsing** tests come with the **`geotiles`** crate later.)
- **Tiny synthetic rasters** (optional): a few **kilobyte-scale** GeoTIFFs **generated in test setup** with GDAL (`gdal` crate: create in-memory or temp **Dataset**, set geotransform + SRS, write a handful of pixels). Keeps “real” GDAL I/O **without** binaries in git. Use for smoke tests only — not a substitute for big real-world files.

### 6.2 Heavy / real GeoTIFFs: do **not** download on every test run

**Avoid** “fetch from the internet on every `cargo test`” as the **default** — it is slow, flaky on CI, and rude to mirror hosts.

**Preferred pattern: download once → cache on disk**

1. **Cache directory** (not in repo), e.g.  
   - `target/geotiles-test-data/` (local, gitignored), or  
   - **`$XDG_CACHE_HOME/geotiles-rs/`** / `~/.cache/geotiles-rs/` (user-wide, persists across clones).
2. **Fixture manifest** in-repo: a small **TOML or JSON** (or Rust `const` URLs + **expected SHA-256**) listing **stable HTTPS URLs** (releases, public buckets, or your own static hosting) for 1–N reference GeoTIFFs.
3. **Test helper** `ensure_fixture(name) -> PathBuf`: if cached file **exists** and **hash matches**, return path; else **download** (with timeout + size cap), verify hash, write to cache, return path.
4. **`cargo test` default:** tests that need heavy fixtures use **`#[ignore]`** **or** **`if std::env::var("GEOTILES_TEST_FETCH").is_ok()`** so plain `cargo test` stays **offline** and **instant**.
5. **Full suite:** document `GEOTILES_TEST_FETCH=1 cargo test -- --include-ignored` (or a dedicated **`--features integration-tests`**) for developers and CI jobs that should hit real files.

### 6.3 Local path override (best developer experience)

- **`GEOTILES_TEST_DATA_DIR`** (or per-file vars): if set, **skip download** and use the user’s **existing** copy (e.g. `~/data/foo.tif`). Tests still **validate** expected CRS/size **bands** if you record metadata in the manifest.

### 6.4 CI

- **Bootstrap** workflows from **§2.1 (imgvwr)** — do not design YAML from scratch.
- **Cache** the GeoTIFF fixture cache directory between runs (e.g. **`actions/cache`** keyed by **URL list + manifest version**). First run downloads; later runs reuse.
- **Optional** job matrix: **with** network + cache vs **without** heavy tests (unit + synthetic only).

### 6.5 Summary

| Approach | Role |
|----------|------|
| **In-repo** | Small tests, synthetic GDAL-generated micro-TIFFs |
| **Cached download** | Real GeoTIFFs from fixed URLs + checksums; **not** every run |
| **Env path** | `GEOTILES_TEST_DATA_DIR` — no download, use local files |
| **`#[ignore]` / env gate** | Keep default `cargo test` fast and offline |
| **Git LFS** | **Not used** |

---

## 7. Phased steps

Complement with tests per **§6**: **unit** tests as modules land; **integration** tests with **cached / env** GeoTIFFs once GDAL pipeline exists.

### 7.0 Mandatory quality gates (before marking any phase or feature done)

Whenever you tick a phase checkbox or declare a **feature complete**, the following **must pass with zero warnings** (do **not** merge or mark done otherwise):

| # | Check | Command (run from **repository root**; **first release** = **`-p libgeotiles`** if `geotiles` is not in the workspace yet) |
|---|--------|-----------------------------------------------------------------------------|
| 1 | **License policy** | `cargo deny check licenses` |
| 2 | **Spell check** | `typos` |
| 3 | **Clippy (all features)** | `cargo clippy -p libgeotiles --all-targets --all-features -- -D warnings` (or `--workspace` when multiple members exist) |
| 4 | **Clippy (no default features)** | `cargo clippy -p libgeotiles --all-targets --no-default-features -- -D warnings` |
| 5 | **Tests (all features)** | `RUSTFLAGS='-D warnings' cargo test -p libgeotiles --all-features` |
| 6 | **Tests (no default features)** | `RUSTFLAGS='-D warnings' cargo test -p libgeotiles --no-default-features` |

After the **`geotiles`** crate is added, run the **same six checks** with **`--workspace`** (or **`-p geotiles`** for binary-only steps) so **both** crates meet **§7.0** — mirror **imgvwr**’s full-matrix style (**§2.1**).

**Notes**

- **`cargo fmt --check`** is recommended on every change; add to pre-commit if you use it.
- **`cargo deny`** requires a committed **`deny.toml`** (add in Phase 0 or first PR that runs deny).
- **`typos`** requires [typos](https://github.com/crate-ci/typos) installed and, if needed, **`.typos.toml`**.
- **`RUSTFLAGS='-D warnings'`** ensures **`cargo test`** does not succeed with **rustc** warnings; without it, only **clippy** is warning-free.
- If a matrix row is **not yet applicable** (e.g. no optional features exist), still run **`--all-features`** / **`--no-default-features`** as soon as the workspace has meaningful feature flags; until then, document the exception in the PR.

**Agent / human contract:** finishing a step = **all six rows green**, not only `cargo build`.

### Phase 0 — Workspace skeleton

- [ ] Root `Cargo.toml`: `[workspace]` with **`libgeotiles`**; `edition = "2024"`. (**Do not** add **`geotiles`** until the **CLI phase** — §1.6, §5.)
- [ ] `libgeotiles`: empty `lib.rs`, `error.rs` with one root `Error`.
- [ ] Relocate **wgpu** (if present) into **`libgeotiles`** as an **optional** dependency behind the **`gpu`** feature only — **not** in workspace `default` features; drop any unused experimental deps from the old single-crate layout.
- [ ] Add **`deny.toml`** (license allow-list) and **`.typos.toml`** so **§7.0** can run once dependencies exist — **prefer copying from [imgvwr](https://github.com/Gigas002/imgvwr)** per **§2.1**, then edit for this repo.
- [ ] Add **`.github/workflows/`** and **`.github/dependabot.yml`** by **mirroring imgvwr** (§2.1): swap crate names, swap system packages for **GDAL**; keep matrices and tooling **otherwise unchanged**.
- **Verify:** `cargo build --workspace`; when tooling is present, **§7.0** gates (may be partially N/A until features land — see §7.0 notes).

### Phase 1 — Errors and GDAL bootstrap

- [ ] `libgeotiles::error`: map `gdal::errors::GdalError` and I/O into `thiserror` variants.
- [ ] Single module to **open** a dataset and read **size**, **geotransform**, **WKT** projection.
- **Verify:** **integration test** or **`examples/`** snippet opens a sample `.tif` and asserts dimensions + origin (or log in test).

### Phase 2 — Coordinates and tile indexing

- [ ] Implement **Web Mercator** tile math (or **geographic** if you choose 4326 tiles — pick one default and document): resolution at `z`, `(lon, lat)` ↔ pixel, **tile (x, y, z)**.
- [ ] TMS: optional **Y flip** when writing paths.
- [ ] Unit tests: known **z/x/y** ↔ bbox corners for a few fixed points.
- **Verify:** `cargo test -p libgeotiles` for `coords` tests.

### Phase 3 — Warp / CRS normalization

- [ ] Implement **warp to EPSG:3857** (or 4326) using GDAL (`gdal::programs::raster::warp` or equivalent stable API for your `gdal` version).
- [ ] Expose **working dataset** handle + geotransform after warp.
- **Verify:** run on a small GeoTIFF; confirm bounds and pixel scale change as expected (log or debug assert).

### Phase 4 — Single tile read + resize + encode

- [ ] Given `(z,x,y)`, compute **source window** in **source pixels** (reuse/refine `get_areas` logic with GDAL’s affine).
- [ ] Read raster band(s) into buffer; resize to `tile_size` with `fast_image_resize` **or** GDAL `RasterIO` with appropriate resampling — **one** primary path.
- [ ] Encode **PNG** via `image` (`encode` module).
- **Verify:** write one tile to `/tmp` and open in an image viewer.

### Phase 5 — Full pipeline + disk output

- [ ] Enumerate all tiles for `[min_z, max_z]` over dataset extent (with optional **crop bbox** args later).
- [ ] **Parallelize** with `rayon` (`par_iter` over tile jobs); use `tracing` for progress.
- [ ] Write tree `{z}/{x}/{y}.{ext}` for the selected default format (e.g. `.png`).
- **Verify:** run on sample GeoTIFF; load folder in a simple Leaflet/OpenLayers test **manually** (outside this repo’s scope).

### Phase 6 — Optional output formats and polish

- [ ] **`libgeotiles::encode`**: trait or enum dispatch **`TileFormat`** → encoder; **PNG** + **JPEG** + **WebP** via **`image`** features (`png`, `jpeg`, `webp`).
- [ ] **AVIF** behind feature **`avif`**: integrate chosen encoder (see §3); document **system** deps if any.
- [ ] **JPEG XL** behind feature **`jxl`**: integrate **`jxl-oxide`** / **`jxl`** (whichever is maintained and ergonomic at implementation time).
- [ ] **Palette / quantization** (`imagequant` / external) — only if needed for size (often PNG-only).
- [ ] Nodata handling, alpha band — align with GDAL dataset semantics; **JPEG** path drops or flattens alpha per §1.5.
- **Verify:** `cargo build -p libgeotiles --features "png,jpeg,webp"` (and separately `--all-features` including `avif`, `jxl` when implemented).

### Phase 7 — GPU tile crop + scale (optional; **migration target**)

**Intent:** This phase delivers the **performance-oriented** path the project **aims** at long-term. It is **not** enabled by default in `Cargo.toml` **defaults**; it **extends** Phases 4–5 without rewriting coordinate or GDAL logic.

- [ ] Add **`wgpu`** + **`pollster`** behind **`gpu`** / `gpu-vulkan` / `gpu-gles` features in **`libgeotiles`** only until **`geotiles`** exists.
- [ ] **`GpuContext`**: one-time device/queue/pool init; log adapter/backend at `info` (same spirit as imgvwr Phase 8).
- [ ] **Upload** per-tile (or batched) source window as **texture**; **compute or render** pipeline outputs **`tile_size × tile_size` RGBA** (or format aligned with encoder).
- [ ] **Readback** to CPU buffer → existing **`encode`** path (§1.5 formats); keep **encode on CPU** unless you explicitly scope GPU encode later.
- [ ] **`pipeline`**: branch on `TileJob` resample backend — CPU path unchanged; GPU path uses shared `(z,x,y)` + window math.
- [ ] **Verify:** `cargo build -p libgeotiles` (default, no GPU); `cargo build -p libgeotiles --features gpu` (or split features); manual or integration test on a machine with Vulkan or GLES; compare **visual** tile output to CPU for a few tiles (exact bytes may differ — document).

**Note:** CI without GPU can still **compile** `--all-features` if software Vulkan (e.g. lavapipe) or GLES is installed — follow **imgvwr’s** GPU matrix pattern in **§2.1**; local verification may be manual.

### Phase 8 — CLI binary + application config (**post–first-release**)

**Prerequisites:** **§9.1** (first library release) done; library API stable enough to wrap.

- [ ] Add **`geotiles`** crate to **`[workspace]`**; **`clap`**, **`tracing-subscriber`**, **`main`** wiring only — call into **`libgeotiles`**.
- [ ] **Config file format** and discovery — **design in this phase** (e.g. TOML, paths, merge order with env/CLI); document in-repo.
- [ ] Map argv + config → **`TileJob`**; exit codes, `--help`, optional completions feature if desired.
- [ ] Extend **§7.0** / CI to **two-crate** **`cargo`** matrices (**§2.1**).
- **Verify:** `cargo run -p geotiles -- --help` and end-to-end run against a real GeoTIFF.

---

## 8. Risk register

| Risk | Mitigation |
|------|------------|
| GDAL version mismatch on user machines | Document **supported GDAL** range; CI images pin distro packages per **§2.1** workflows |
| Large rasters exhaust RAM | Stream windows per tile; avoid holding full raster |
| Double resample (warp + resize) blurs | Use GDAL with **appropriate overview** or single resample stage where possible |
| TMS/XYZ confusion | One well-tested helper + explicit flag |
| GPU **PCIe readback** cost negates wins | Profile; batch tiles; minimize readback size; document when GPU helps (large zoom jobs, many tiles) |
| No Vulkan/GLES on host | Library **falls back to CPU** or returns **error** per policy; future CLI may expose `--backend` (§5) |
| AVIF/JXL **build complexity** (native deps, long compile) | Keep behind **features**; document **optional** packager deps; prefer **pure-Rust** where it meets quality/perf |
| **Network tests** flaky or slow | Do **not** download on every `cargo test`; use **§6** cache + checksums + `#[ignore]` / env gate |
| **Fixture URL** moved or changed | Version the manifest; pin **SHA-256**; update URLs in one commit |
| **`cargo-deny` / `typos` not installed** locally | Document install (e.g. `cargo install cargo-deny typos-cli`); CI should run §7.0 when added |

---

## 9. Definition of done

### 9.1 First release — **`libgeotiles` only** (§1.6)

- [ ] **§7.0 quality gates** pass for **`-p libgeotiles`** (license deny, typos, clippy both feature matrices, tests both matrices with `RUSTFLAGS='-D warnings'`).
- [ ] `cargo build --release -p libgeotiles` with default features.
- [ ] Library can produce a **valid** `{z}/{x}/{y}` tree (via API or **examples**/integration tests) from a **representative** GeoTIFF for at least **EPSG:4326** and **EPSG:3857** sources (after warp).
- [ ] **No** dependency on abandoned crates (per §3 policy).
- [ ] **No** requirement for Avalonia, Docker, or GTiff2Tiles.Console parity.
- [ ] **GPU path (Phase 7):** optional `gpu` features compile; CPU remains **default** features; optional manual/visual check vs CPU path for a few tiles.
- [ ] **Output formats (Phase 6):** at least **PNG** on default features; **JPEG**, **WebP**, **AVIF**, **JXL** via **optional** features with **documented** encoder choices and packager notes.
- [ ] **Testing (§6):** fixture manifest + cache helper; default `cargo test` **offline**; documented command for **ignored** / **fetch** integration tests; optional **synthetic** micro-GeoTIFF tests in-repo.
- [ ] **Repo automation (§2.1):** `.github/workflows`, Dependabot, `deny.toml`, `.typos.toml` aligned with **imgvwr** (minimal renames + GDAL; **`-p libgeotiles`** in jobs until **`geotiles`** exists).

### 9.2 Post–first-release — **CLI + config** (Phase 8, §5)

- [ ] **`geotiles`** binary crate added; **§7.0** extended to **full workspace** (or **`-p geotiles`** where appropriate).
- [ ] **Application config** format chosen and documented (e.g. TOML); resolution order defined.
- [ ] End-to-end: **`geotiles`** produces tiles from argv + config on a real GeoTIFF.

---

## 10. Document maintenance

Update this file when: workspace layout changes, a phase is completed (checkboxes), dependency strategy changes, CRS/tile-default decisions, **release scope** (§1.6, §9), **CLI/config** (§5, §9.2), **testing/fixture policy** (§6), **quality gate commands** (§7.0), or **imgvwr template alignment** (§2.1) changes.

### Revision history

| Date | Change |
|------|--------|
| 2026-04-20 | Initial plan: core + CLI only; deps; phased steps; explicit non-goals |
| 2026-04-20 | GPU tile crop/scale as optional migration target (`wgpu`); CPU default features + default CLI backend; Phase 8; §1.4 policy |
| 2026-04-20 | Optional output formats: PNG, JPEG, WebP, AVIF, JXL (§1.5); `encode/` module; Phase 7 expanded; Cargo features |
| 2026-04-20 | §6 Testing strategy: real GeoTIFFs without LFS; cache + checksums; env override; not downloading every test; phased steps → §7 |
| 2026-04-20 | §7.0 Mandatory gates: `cargo deny`, `typos`, clippy `--all-features` / `--no-default-features`, `cargo test` both with `RUSTFLAGS='-D warnings'` |
| 2026-04-20 | §2.1: docs + CI/tooling organization mirrors [imgvwr](https://github.com/Gigas002/imgvwr) mostly unchanged; §1.2 non-goals adjusted; Phase 0 + §6.4 + DoD |
| 2026-04-20 | Library-first release (§1.6, §9.1); CLI + config deferred (§5, Phase 8, §9.2); workspace starts `libgeotiles`-only; §7.0 `-p libgeotiles`; phases renumbered |
