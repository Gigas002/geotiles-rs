# GTiff2Tiles → Rust (`geotiles-rs`) migration plan

This document is both a **human roadmap** and an **agent playbook**: steps are sized for focused implementation sessions, end in a **verified** state (`cargo build`, `cargo fmt`, `cargo clippy`), and state **how to verify**. It follows the structure and discipline of example plans ([tofi-rs `RUST_MIGRATION_PLAN`](https://raw.githubusercontent.com/Gigas002/tofi-rs/refs/heads/v0/docs/RUST_MIGRATION_PLAN.md), [`POST_MIGRATION_PLAN`](https://raw.githubusercontent.com/Gigas002/tofi-rs/refs/heads/v0/docs/POST_MIGRATION_PLAN.md), [imgvwr `IMV_RS_PLAN`](https://raw.githubusercontent.com/Gigas002/imgvwr/19f5e82b6a5cc7b23e2bf25e03ca448b1d8fb109/docs/IMV_RS_PLAN.md)).

**Primary scope:** **`libgeotiles`** — the library, its API, encoders, GDAL/GPU pipeline, tests, and repo tooling (**§2.1**). **CLI binary (`geotiles`), command-line flags, and application config file format are explicitly post–first-release** (see **§1.6** and **§5**).

**Reference product:** [Gigas002/GTiff2Tiles](https://github.com/Gigas002/GTiff2Tiles) — C# library analogous to `gdal2tiles.py` / MapTiler: GeoTIFF → web map tiles (zoom levels, slippy-map layout, CRS handling). The **C# codebase is behavioral reference only**, not an API or architecture spec.

---

## 1. Goals and constraints

### 1.1 Goals

- **Same problem domain** as GTiff2Tiles **Core**: read GeoTIFF (and similar GDAL rasters), optionally reproject, compute **Web Mercator** or **WGS84 geographic** tile grids, **crop/resample** per tile, **encode** tiles, write to `{z}/{x}/{y}` layout with optional **TMS vs XYZ** indexing.
- **Performance and simplicity are the primary design principles.** Every API, module boundary, and dependency choice must be evaluated against these two criteria first. The original GTiff2Tiles was created because `gdal2tiles.py` / GDAL's built-in tiling is too slow; this library must be measurably faster on the same workloads (see **§6.6**).
- **Tile crop happens inside the library — always.** Per-tile crop + resample is the **core value** of `libgeotiles`; it must **never** shell out to an external tool or delegate to `gdal2tiles`. See **§4**.
- **Chunked / streaming I/O:** input rasters can be arbitrarily large (200 GB+ GeoTIFFs are a real use-case). The pipeline must **never** load the full raster into RAM at once. A configurable `chunk_size` (e.g. pixel rows, byte budget, or source-pixel area) on `GeoTiff` controls how large a source-pixel window is read into RAM at one time. Tiles whose source windows fall within the current chunk are processed and flushed to disk before the buffer is released and the next chunk is read. On the **GPU path**, the same budget governs VRAM staging: the GPU buffer is freed before the next chunk is uploaded (see **§1.4**, **§4**). A sensible default must be provided so callers that do not set `chunk_size` still behave safely on large inputs.
- **Logging via `tracing` is a first-class concern throughout all phases.** Spans and events must be added **as each module is implemented**, not retrofitted at the end. Every phase must include logging for its new code paths (see **§4**, Phase 1, and **§9.1**).
- **Optional tile output formats** (see **§1.5**): **PNG** and **JPEG** as the baseline set; **WebP**, **AVIF**, and **JPEG XL** as **opt-in** Cargo features selected via **library API** (`TileFormat`, build flags); heavy or native-backed codecs stay **optional**. A future CLI will map user input to these types — **not** part of the first release.
- **First release focus:** **`libgeotiles` only** — public API (`GeoTiff`, `TileFormat`, `ResampleBackend`), pipeline, encoders, optional GPU path, tests, docs in-repo, CI (**§2.1**). **No** shipped CLI binary, **no** committed application-level config schema in v1.
- **Clean-room design:** implement **equivalent functionality** in the **simplest, fastest** way that fits Rust + GDAL. **Do not** mirror C# class hierarchy, exception types, or method signatures.
- **Rust edition:** `2024` in `[workspace.package]` and member crates (align with current ecosystem practice).
- **Repository layout for docs and CI:** follow [**imgvwr**](https://github.com/Gigas002/imgvwr) **with minimal changes** — see **§2.1** (workflows, Dependabot, `deny.toml`, `.typos.toml`, `docs/` conventions). Placeholder files already copied from [tofi-rs](https://github.com/Gigas002/tofi-rs); Phase 0 adapts them.
- **Dependency policy:** prefer crates with **recent releases or maintenance** (roughly **within one year** at dependency lock time). **Reject** abandoned crates; re-evaluate when bumping `Cargo.lock`.
- **Testing:** aim for **broad automated coverage**: pure logic (**unit**), GDAL-backed **integration** tests, and **end-to-end** runs on **real** GeoTIFFs. Large rasters **must not** live in git (and **Git LFS is not used**). Follow **§6** for how to obtain, cache, and optionally skip heavy assets.
- **Benchmarks:** a `criterion`-based suite (see **§6.6**) tracks performance of the tile crop/resample/encode hot path, CPU vs GPU, and competing resample libraries. Results are recorded in-repo and re-run whenever the hot path changes.
- **Resampling / tile "crop" path:** the **intended end-state** is an **optional GPU pipeline** (**wgpu**) for **per-tile crop + scale**. **Shipping defaults:** **`default` features = CPU-only**. **GPU is opt-in** via Cargo feature(s) (e.g. `gpu` / `gpu-vulkan` / `gpu-gles`) and a **library-level** choice via `GeoTiff::backend()` (e.g. `ResampleBackend::Cpu` vs `ResampleBackend::Gpu`). A **future** CLI may expose `--backend` — **out of scope for first library release** (§1.6).

### 1.2 Non-goals (explicitly out of scope for this document)

- **Standalone documentation product** (public **GitHub Pages** site, **Wiki**, packaged **man pages**) as a migration deliverable — **not required**; API documentation is **handled automatically by [docs.rs](https://docs.rs) on crate publish**. **In-repo** docs follow **§2.1**.
- **Inventing CI / repo tooling layout from scratch** — **avoid**. Placeholder CI/tooling files have already been **copied from [tofi-rs](https://github.com/Gigas002/tofi-rs)** and committed to this repo; **Phase 0** covers adapting them (crate names, system packages, project-specific exclusions) after the initial workspace and crate are initialized — see **§7 Phase 0**. Use [**imgvwr**](https://github.com/Gigas002/imgvwr) as the style reference throughout (§2.1). **Docker**, **NuGet**, **codecov** flags: only if the template already has an equivalent pattern worth mirroring; otherwise skip.
- **Avalonia / GUI** or any desktop UI.
- **Line-for-line** port of **C#** tests. **First release:** **no** CLI crate, **no** user-facing config file — those are **post–first-release** (§1.6, §5). **Rust** tests for the library should still be **comprehensive** (see **§6**), including real-world GeoTIFFs via **out-of-repo** assets.
- **Pixel-identical** output vs C# or `gdal2tiles.py` for every edge case — document intentional differences if any.

### 1.3 What “parity” means here

| Area               | GTiff2Tiles Core (reference)                                           | Target in Rust                                                                                                                                                                                                                                                                         |
| ------------------ | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GDAL               | `GdalWarp`, `GDALInfo`, geo transform, projection strings              | `gdal` crate: open dataset, warp options, read windows, CRS metadata                                                                                                                                                                                                                   |
| Fast image / tiles | **NetVips** (`Image`, tile cache, parallel crops)                      | **Baseline (default):** GDAL window read + **`fast_image_resize`** (or GDAL resampling) + **encode** (see §1.5). **Target (optional `gpu`):** **`wgpu`** crop + scale → **readback** → **CPU encode** (same format set as CPU path).                                                   |
| Coordinates        | `GeodeticCoordinate`, `MercatorCoordinate`, `Number` (x,y,z), TMS flag | Small **Rust types** + **pure functions** (see §3)                                                                                                                                                                                                                                     |
| Orchestration      | `TileGenerator`, `Raster`, `RasterTile`                                | **`GeoTiff`** (`src/geotiff.rs`): builder-style struct; `GeoTiff::open(path)` → configure → `GeoTiff::crop()` runs the full pipeline. Internal `libgeotiles::pipeline` handles chunk loop + `rayon` parallelism; **`ResampleBackend`** enum (`Cpu` / `Gpu`) selects the resample path. |

### 1.4 CPU vs GPU (policy)

|                | **CPU (default)**                                                   | **GPU (optional, migration target)**                                                                                                     |
| -------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| **Cargo**      | In `default` features                                               | Separate feature(s), e.g. `gpu`, `gpu-vulkan`, `gpu-gles`; future **`geotiles`** crate **forwards** the same names (**§1.6**)            |
| **Runtime**    | Always available                                                    | Only if built with GPU features **and** `GeoTiff::backend(ResampleBackend::Gpu)` is set; **future** CLI may add `--backend gpu` (**§5**) |
| **Work split** | GDAL → **chunk buffer** (bounded by `chunk_size`) → resize → encode | GDAL → **chunk buffer** (bounded by `chunk_size`) → GPU upload → crop/scale → readback → encode → **free VRAM before next chunk**        |
| **Failure**    | N/A                                                                 | If GPU init fails, **fall back to CPU** with `tracing::warn!` (or return error — pick one policy and document it)                        |

Design `GeoTiff` and the internal tile-step APIs so the **same** `(z,x,y)` math and **output bytes** contract works for both backends; only the **resample implementation** swaps.

### 1.6 First release vs post-release (CLI and config)

| Milestone                                            | In scope                                                                                                                                                                                                                       | Out of scope (defer)                                                                                                                                             |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **First release (`libgeotiles` v0.x / 1.0 library)** | Crate **`libgeotiles`**, stable-enough API for tiling jobs, encoders, CPU + optional GPU pipeline, tests, **§7.0** gates on **`-p libgeotiles`**, CI (**§2.1**) targeting the library                                          | **`geotiles`** binary, **`clap`**, argv parsing, **`tracing-subscriber` wiring in a `main`**, **application config file** (TOML/YAML/etc.), env-file conventions |
| **Post–first-release phases**                        | Add **`geotiles`** workspace member (or separate step), **CLI design from scratch**, **config format** (likely **TOML** — decide when implementing), loading order (defaults → file → env → CLI), shell completions if desired | —                                                                                                                                                                |

**Rule:** Do **not** block the library release on CLI or config decisions. **`GeoTiff`** and related types should be **CLI-agnostic** so a later binary only **constructs** them from parsed args + config.

### 1.5 Output formats (optional; Cargo features + library API)

Support **multiple** container/codec choices; **not** all need to be in `default` features.

| Format      | Extension(s)    | Typical role                                  | Cargo / notes                                                                                                                                                                                                                   |
| ----------- | --------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **PNG**     | `.png`          | Lossless, universal; **default** output       | `image` / `png` feature                                                                                                                                                                                                         |
| **JPEG**    | `.jpg`, `.jpeg` | Photos, smaller than PNG; lossy               | `image` / `jpeg` feature                                                                                                                                                                                                        |
| **WebP**    | `.webp`         | Lossy or lossless; good for web maps          | `image` / `webp` feature                                                                                                                                                                                                        |
| **AVIF**    | `.avif`         | Modern lossy/lossless; smaller at cost of CPU | `image` **and/or** dedicated encoder crate — **verify** at implementation time that chosen stack is **maintained**; may pull **system** `libavif` or use a **pure-Rust** path (e.g. `ravif` + `dav1d`) — document packager deps |
| **JPEG XL** | `.jxl`          | High efficiency; growing viewer support       | Often **`jxl-oxide`** (encode) or **`jxl`** — **not** always via `image`; gate behind feature `jxl`                                                                                                                             |

**Rules**

- **`default` library features:** include at least **PNG** (and optionally **JPEG** if you want “one lossy” out of the box — pick one policy).
- **Library:** `TileFormat` / encoder choice must respect **compiled-in** features; return a clear **error** if a format was requested but the feature is off.
- **Alpha / nodata:** PNG/WebP/AVIF/JXL can carry alpha; JPEG cannot — document **flatten** or **drop alpha** behavior for `.jpg`.
- **Quality:** store optional **quality** on `GeoTiff` (builder setter) for lossy formats — a **future** CLI may map `--quality`; not required for first release beyond API support if you want it.

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
      geotiff.rs                # GeoTiff — primary public struct; open() + builder setters + crop() entry point
      crs/                      # CRS detection, EPSG:4326 / EPSG:3857 helpers (thin wrapper over GDAL)
      coords/                   # tile indices, bbox ↔ pixels, TMS/XYZ flip
      gdal_io/                  # internal: warp, read_raster windowed reads, geotransform helpers
      tile/                     # internal: single-tile window extract, resample, encode bytes
      tile/gpu.rs               # optional: wgpu context, pipelines, readback (behind `feature = "gpu"`)
      pipeline/                 # internal: zoom range, tile enumeration, chunk loop; dispatches Cpu vs Gpu
      pipeline/chunks.rs        # chunk iterator: groups tiles by source-pixel window, drives read/flush loop
      output/                   # directory writer, path pattern `{z}/{x}/{y}.ext`
      encode/                   # RGBA buffer → bytes: dispatch png / jpeg / webp / avif / jxl by `TileFormat`
    tests/
      fixtures_manifest.toml    # optional: stable URLs + SHA-256 for heavy GeoTIFFs (see §6); not the rasters themselves
    examples/                   # optional: small binaries that open a GeoTiff and call .crop() (dogfood before CLI — §5)
  # geotiles/                   # POST first library release — see §5
```

**Naming (first release)**

| Role    | Cargo `package.name` | Rust crate id |
| ------- | -------------------- | ------------- |
| Library | `libgeotiles`        | `libgeotiles` |

**Naming (post-release)**

| Role | Cargo `package.name` | Installed binary |
| ---- | -------------------- | ---------------- |
| CLI  | `geotiles`           | `geotiles`       |

### 2.1 Repository organization — **imgvwr** as canonical template (keep structure **mostly unchanged**)

**Reference repo:** [**Gigas002/imgvwr**](https://github.com/Gigas002/imgvwr) (use default branch or the branch you treat as current for Rust workspace work).

**Copy and adapt with minimal edits** so `geotiles-rs` stays organized like imgvwr:

| Area                         | Action                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **`.github/workflows/`**     | Mirror workflow **names**, **matrix style**, **`dtolnay/rust-toolchain`**, **`Swatinem/rust-cache`**, job split (**build** / **fmt-clippy** / **test** / **typos** / **deny** / **deploy** if present). **Until the `geotiles` binary exists**, use **`-p libgeotiles`** (or `--workspace` with a single member) in all **`cargo`** invocations; when adding **`geotiles`**, extend commands to match **imgvwr**’s two-crate pattern (`libimgvwr` → `libgeotiles`, `imgvwr` → `geotiles`). |
| **`.github/dependabot.yml`** | Copy structure; set `directory: "/"` for the workspace root.                                                                                                                                                                                                                                                                                                                                                                                                                               |
| **`deny.toml`**              | Copy **license policy** and structure; adjust crate names / exceptions only if `cargo deny` requires it for GDAL-related SPDX.                                                                                                                                                                                                                                                                                                                                                             |
| **`.typos.toml`**            | Copy; **extend** `extend-exclude` for GeoTIFF paths, `target/`, cache dirs, and any GDAL-specific false positives as they appear.                                                                                                                                                                                                                                                                                                                                                          |
| **Root `Cargo.toml`**        | Align **`[workspace.package]`** patterns (edition, license metadata, repository URL) with imgvwr style — point `repository` / `homepage` to **this** repo.                                                                                                                                                                                                                                                                                                                                 |
| **`docs/`**                  | Keep this migration plan (and any small companion docs) in the same **spirit** as imgvwr’s `docs/` (plan + revision history); **do not** require a separate published site.                                                                                                                                                                                                                                                                                                                |

**System packages in CI images:** **Replace** imgvwr’s Wayland / xkb / (optional) libavif with what **this** project needs — at minimum **`libgdal`** / **`gdal`** via distro packages (`libgdal-dev`, `pkg-config`, build-essential). For **`--all-features`** GPU jobs, keep imgvwr’s **Mesa / Vulkan (lavapipe)** pattern if you mirror the **gpu-vulkan** matrix entry. Document replacements in **workflow comments** so the next maintainer sees the diff vs imgvwr.

**Rule:** When adding or changing automation, **open imgvwr side-by-side** and preserve **file layout and naming** unless there is a **project-specific** reason to diverge.

---

## 3. Dependencies (candidates — pin **current latest** at implementation time)

**Policy:** use **two-component** version requirements in `Cargo.toml` (e.g. `0.19`) where practical; exact versions live in **`Cargo.lock`**. Before each release, run `cargo update` and **confirm** each crate still shows activity within ~12 months (crates.io / GitHub). **Do not** depend on unmaintained crates.

**GeoRust ecosystem reference:** [**https://georust.org/**](https://georust.org/) — canonical index of maintained Rust geospatial crates (GDAL bindings, `geo`, `proj`, `geozero`, etc.). Consult this page when evaluating or adding geospatial dependencies.

| Crate                                                               | Role                                                                           | Notes                                                                                                                                                      |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [**gdal**](https://crates.io/crates/gdal)                           | GDAL Dataset, warping, `read_raster`, geotransform, SRS                        | System **libgdal** required; primary geospatial engine                                                                                                     |
| [**thiserror**](https://crates.io/crates/thiserror)                 | `Error` enums in `libgeotiles`                                                 |                                                                                                                                                            |
| [**image**](https://crates.io/crates/image)                         | Encode **PNG**, **JPEG**, **WebP** via selective features                      | `default-features = false`; enable `png`, `jpeg`, `webp` as needed; **AVIF** via `image` only if you accept its **native** / feature story at lock time    |
| **AVIF encoder** (TBD at implementation)                            | **`.avif`** tiles                                                              | e.g. **`ravif`**, or **`image`** with `avif` + system libs — pick **one** maintained path; re-check §3 health policy                                       |
| **JPEG XL encoder** (TBD at implementation)                         | **`.jxl`** tiles                                                               | e.g. **`jxl-oxide`** encode API or **`jxl`** — often **outside** `image`; separate feature `jxl`                                                           |
| [**fast_image_resize**](https://crates.io/crates/fast_image_resize) | SIMD-friendly resize to tile size                                              | Alternative: GDAL overview/warp only — pick one path to avoid double work                                                                                  |
| [**rayon**](https://crates.io/crates/rayon)                         | Parallel tile generation                                                       | Optional `features` gate if you want single-threaded builds                                                                                                |
| [**tracing**](https://crates.io/crates/tracing)                     | Structured logs in library                                                     | **`tracing-subscriber`** only when a **`main`** exists (CLI phase) or **dev** tests                                                                        |
| [**clap**](https://crates.io/crates/clap)                           | CLI                                                                            | **Post–first-release** — **`geotiles`** binary only (§1.6)                                                                                                 |
| [**memmap2**](https://crates.io/crates/memmap2)                     | Optional: mmap large reads for chunked I/O hot path                            | Evaluate after chunked reader (§4 step 4) is in place; only adopt if profiling on large GeoTIFFs shows meaningful gain over GDAL `RasterIO` windowed reads |
| [**wgpu**](https://crates.io/crates/wgpu)                           | **Optional (`gpu` feature):** crop + resize on GPU                             | `default-features = false`; enable `wgsl` + one backend (`vulkan` and/or `gles`)                                                                           |
| [**pollster**](https://crates.io/crates/pollster)                   | **Optional:** block on async `wgpu` init / submit without a full async runtime | Same pattern as imgvwr GPU phases                                                                                                                          |

**Optional / later**

| Crate                                               | Role                                                                               |
| --------------------------------------------------- | ---------------------------------------------------------------------------------- |
| [**geo-types**](https://crates.io/crates/geo-types) | `Rect`, `Coord` — only if you want interop; plain `f64` pairs may suffice          |
| [**proj**](https://crates.io/crates/proj)           | PROJ bindings — **avoid duplicating GDAL** unless you need transforms without GDAL |

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

**Primary public type: `GeoTiff` (`src/geotiff.rs`)**

`GeoTiff` is the single entry point callers interact with. It uses a consuming builder pattern: `GeoTiff::open(path)?` returns a configured-with-defaults instance; each setter (`.zoom()`, `.chunk_size()`, `.format()`, `.output()`, `.backend()`) returns `Self`; `.crop()` consumes the value and executes the full pipeline. All internal modules (`gdal_io/`, `pipeline/`, `tile/`, `encode/`, `output/`) are implementation details — only `GeoTiff`, `TileFormat`, `ResampleBackend`, and error types are public.

```rust
// illustrative — exact API decided at implementation time
GeoTiff::open("big.tif")?
    .zoom(4..=12)
    .chunk_size(512)        // source-pixel rows per RAM window
    .format(TileFormat::Png)
    .output("tiles/")
    .backend(ResampleBackend::Cpu)  // or ::Gpu behind feature
    .crop()?;
```

Implement **features**, not **C# types**:

1. **Open source** — path in → `Dataset` (read-only), band count, dtype, nodata.
2. **Working CRS** — normalize to **EPSG:3857** (typical web maps) or **EPSG:4326** via GDAL warp to a **temporary** or **in-memory** dataset (strategy: temp GeoTIFF vs `VRT` — choose simplest robust option).
3. **Extent** — from geotransform + size, in the **working CRS**; helpers for **tile index range** for given `z`, tile size (256 default), **TMS** y-order flag.
4. **Chunked read manager** — the source raster is **never** loaded fully into RAM. The `chunk_size` setter on `GeoTiff` (e.g. maximum source-pixel rows, or a byte budget) controls the read window. The pipeline (`pipeline/chunks.rs`) groups tiles by which source chunk they overlap, processes all tiles whose windows fall within the current chunk, writes them to disk, then releases the RAM buffer (or VRAM buffer on the GPU path — see §1.4) and advances to the next chunk. A sensible built-in default must be provided so naive callers are safe on arbitrarily large inputs without explicitly setting `chunk_size`. `tracing::debug!` must log chunk boundaries and buffer sizes.
5. **Per-tile pipeline** — for `(z, x, y)` within the current chunk: compute **source pixel window** (and subpixel bounds), read from the chunk buffer already in RAM (no second GDAL call), **crop and resample entirely inside `libgeotiles`** to `tile_size × tile_size` (**CPU** or **GPU** per §1.4) — this is the **core reason** this library exists; `gdal2tiles.py` / GDAL's own tiling is too slow for production use and the original GTiff2Tiles was created specifically to replace it. The crop step **must never** shell out to an external tool. **Encode** to bytes on CPU using **`TileFormat`** (§1.5): **png**, **jpeg**, **webp**, **avif**, **jxl** as enabled by features.
6. **Output** — write files under `output/{z}/{x}/{y}.{ext}` matching the selected format; optional **metadata** file (e.g. simple `bounds` JSON) — **minimal**, only if needed for web viewers; skip gdal2tiles' full XML suite unless required.
7. **Logging** — every step above must emit **`tracing`** spans / events at appropriate levels (`debug` for per-tile detail and chunk boundaries, `info` for phase transitions, `warn` for fallbacks, `error` for failures). Log points are added **as each step is implemented**, not retrofitted later (see Phase 1 and §1.1).

**Reuse ideas from current repo:** `main.rs` already sketches **resolution**, **pixel/tile numbers**, and **`get_areas`**-style read/write regions — **refactor into `libgeotiles::coords`** and validate against GDAL geotransform math (do not trust duplicated formulas without tests against GDAL).

---

## 5. CLI and application config — **post–first-release** (not part of initial library milestone)

**Status:** **Deferred** until **`libgeotiles`** reaches the **first release** criteria (**§9.1**). Design the **`geotiles`** binary and **user-facing config** in a **separate** planning pass so the **library API** stays stable and **CLI-agnostic**.

**Rough direction** (non-binding — revisit when starting this phase):

- **`geotiles`** crate: **`clap`** for argv, thin **`main`**, **`tracing-subscriber`** for logs.
- **Config file:** format **TBD** (often **TOML**); resolution order **TBD** (e.g. XDG config dir + `--config` override). Must map cleanly onto **`GeoTiff`** builder setters — **no** business logic in the binary beyond parsing and wiring.
- Flags (illustrative only): input GeoTIFF/VRT, output directory, zoom range, tile size, TMS/XYZ, **`--format`**, **`--quality`**, **`--threads`**, **`--backend cpu|gpu`** when GPU feature is on, etc.

**First-release substitute for dogfooding:** **`examples/`** binaries or **integration tests** that call `GeoTiff::open(...).crop()` directly — no separate config file required.

---

## 6. Testing strategy and GeoTIFF fixtures (no Git LFS)

Large GeoTIFFs **cannot** be committed to the repo. **Git LFS is not an option.** Use a **layered** approach so `cargo test` is **fast and offline-friendly by default**, while still allowing **full** validation when assets and network are available.

### 6.0 Test file architecture (mandatory)

**Rule: tests must never live inside source files.** No `#[cfg(test)]` blocks embedded in `.rs` modules.

| Test kind         | Location                                          | Example                                  |
| ----------------- | ------------------------------------------------- | ---------------------------------------- |
| **Unit tests**    | Sibling `tests.rs` next to the module under test  | `src/gdal_io/mod.rs` → `src/gdal_io/tests.rs` |
| **Integration tests** | `libgeotiles/tests/` (one file per concern)   | `tests/gdal_io.rs`                       |

Wire unit test files with `#[cfg(test)] mod tests;` at the bottom of the module — in the **module file**, pointing to the sibling `tests.rs`. The test logic itself lives only in `tests.rs`.

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

| Approach                   | Role                                                         |
| -------------------------- | ------------------------------------------------------------ |
| **In-repo**                | Small tests, synthetic GDAL-generated micro-TIFFs            |
| **Cached download**        | Real GeoTIFFs from fixed URLs + checksums; **not** every run |
| **Env path**               | `GEOTILES_TEST_DATA_DIR` — no download, use local files      |
| **`#[ignore]` / env gate** | Keep default `cargo test` fast and offline                   |
| **Git LFS**                | **Not used**                                                 |

### 6.6 Benchmarks

Benchmarks are a **first-class deliverable** — not optional polish. They exist to **measure the effect** of every major implementation choice (resampling library, parallelism tuning, GPU offload, etc.) and to prevent performance regressions.

**Tooling:** [**`criterion`**](https://crates.io/crates/criterion) in `libgeotiles/benches/`; gated behind the normal `[[bench]]` Cargo target so they do not slow down `cargo test`.

**Benchmark targets to establish (add as the relevant phase lands):**

| Benchmark                           | Phase added | What it measures                                                                                                                                                                                           |
| ----------------------------------- | ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bench_tile_encode_{png,jpeg,webp}` | Phase 6     | Encoder throughput for a fixed RGBA buffer → bytes                                                                                                                                                         |
| `bench_tile_resample_cpu`           | Phase 4     | `fast_image_resize` (or GDAL) resize from source window → tile size                                                                                                                                        |
| `bench_pipeline_cpu_zoom{z}`        | Phase 5     | Full CPU pipeline: open → warp → N tiles at zoom `z`                                                                                                                                                       |
| `bench_pipeline_gpu_zoom{z}`        | Phase 7     | Same tile set via GPU path; compare directly to CPU baseline                                                                                                                                               |
| `bench_resample_lib_comparison`     | Phase 4–5   | Side-by-side of candidate resize libraries (e.g. `fast_image_resize` vs GDAL resampling) for the same input; results inform the default choice                                                             |
| `bench_chunk_size_sweep`            | Phase 5     | Full pipeline run at varying `chunk_size` values (e.g. 64, 256, 1024 rows) on a fixed GeoTIFF; reveals the sweet spot between RAM use and throughput (fewer GDAL reads = faster, but larger RAM footprint) |

**Rules**

- **Record baseline results** (wall time, throughput) in `docs/benchmarks/` as Markdown or CSV snapshots when a path is first stabilized — do **not** rely solely on Criterion's local HTML report.
- **Re-run** benchmarks before and after any change that touches the hot path (resample, encode, pipeline parallelism, GPU context).
- **CI:** do **not** run full benchmarks on every PR (too slow); add an **optional** `bench` workflow (manual trigger or `[bench]` commit tag) that runs on a consistent self-hosted or pinned runner for reproducibility.
- **GPU vs CPU comparison:** once Phase 7 lands, document the crossover point — tile count / zoom level at which GPU overhead pays off — in `docs/benchmarks/`.

---

## 7. Phased steps

Complement with tests per **§6**: **unit** tests as modules land; **integration** tests with **cached / env** GeoTIFFs once GDAL pipeline exists.

### 7.0 Mandatory quality gates (before marking any phase or feature done)

Whenever you tick a phase checkbox or declare a **feature complete**, the following **must pass with zero warnings** (do **not** merge or mark done otherwise):

| #   | Check                            | Command (run from **repository root**; **first release** = **`-p libgeotiles`** if `geotiles` is not in the workspace yet) |
| --- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 1   | **License policy**               | `cargo deny check licenses`                                                                                                |
| 2   | **Spell check**                  | `typos`                                                                                                                    |
| 3   | **Clippy (all features)**        | `cargo clippy -p libgeotiles --all-targets --all-features -- -D warnings` (or `--workspace` when multiple members exist)   |
| 4   | **Clippy (no default features)** | `cargo clippy -p libgeotiles --all-targets --no-default-features -- -D warnings`                                           |
| 5   | **Tests (all features)**         | `RUSTFLAGS='-D warnings' cargo test -p libgeotiles --all-features`                                                         |
| 6   | **Tests (no default features)**  | `RUSTFLAGS='-D warnings' cargo test -p libgeotiles --no-default-features`                                                  |

After the **`geotiles`** crate is added, run the **same six checks** with **`--workspace`** (or **`-p geotiles`** for binary-only steps) so **both** crates meet **§7.0** — mirror **imgvwr**’s full-matrix style (**§2.1**).

**Notes**

- **`cargo fmt --check`** is recommended on every change; add to pre-commit if you use it.
- **`cargo deny`** requires a committed **`deny.toml`** (add in Phase 0 or first PR that runs deny).
- **`typos`** requires [typos](https://github.com/crate-ci/typos) installed and, if needed, **`.typos.toml`**.
- **`RUSTFLAGS='-D warnings'`** ensures **`cargo test`** does not succeed with **rustc** warnings; without it, only **clippy** is warning-free.
- If a matrix row is **not yet applicable** (e.g. no optional features exist), still run **`--all-features`** / **`--no-default-features`** as soon as the workspace has meaningful feature flags; until then, document the exception in the PR.

**Agent / human contract:** finishing a step = **all six rows green**, not only `cargo build`.

### Phase 0 — Workspace skeleton

- [x] Root `Cargo.toml`: `[workspace]` with **`libgeotiles`**; `edition = "2024"`. (**Do not** add **`geotiles`** until the **CLI phase** — §1.6, §5.)
- [x] `libgeotiles`: empty `lib.rs`, `error.rs` with one root `Error`.
- [x] Relocate **wgpu** (if present) into **`libgeotiles`** as an **optional** dependency behind the **`gpu`** feature only — **not** in workspace `default` features; drop any unused experimental deps from the old single-crate layout.
- [x] **Adapt CI / tooling placeholder files** (already copied from [tofi-rs](https://github.com/Gigas002/tofi-rs) into this repo): update **`deny.toml`**, **`.typos.toml`**, **`.github/workflows/`**, and **`.github/dependabot.yml`** for this project — swap crate names to `libgeotiles`, replace any Wayland / display system packages with **`libgdal-dev`** + build tools, remove tofi-specific exceptions, keep matrices and job structure otherwise unchanged (§2.1). Use [**imgvwr**](https://github.com/Gigas002/imgvwr) as the style reference when resolving ambiguities.
- **Verify:** `cargo build --workspace`; when tooling is present, **§7.0** gates (may be partially N/A until features land — see §7.0 notes).

### Phase 1 — Errors, GDAL bootstrap, and logging foundation

- [x] `libgeotiles::error`: map `gdal::errors::GdalError` and I/O into `thiserror` variants.
- [x] Single module to **open** a dataset and read **size**, **geotransform**, **WKT** projection.
- [x] Add **`tracing`** as a dependency; instrument the dataset-open path with `tracing::debug!` / `tracing::info!` spans from the very start — **logging must grow with every subsequent phase**, not be retrofitted at the end. Add `tracing-subscriber` as a **`dev-dependency`** only (for test output); wire it up in integration tests / examples for observability during development.
- **Verify:** **integration test** or **`examples/`** snippet opens a sample `.tif` and asserts dimensions + origin; tracing events are visible when `RUST_LOG=debug` is set.

### Phase 2 — Coordinates and tile indexing

- [x] Implement **Web Mercator** tile math (or **geographic** if you choose 4326 tiles — pick one default and document): resolution at `z`, `(lon, lat)` ↔ pixel, **tile (x, y, z)**.
- [x] TMS: optional **Y flip** when writing paths.
- [x] Unit tests: known **z/x/y** ↔ bbox corners for a few fixed points.
- **Verify:** `cargo test -p libgeotiles` for `coords` tests.

### Phase 3 — Warp / CRS normalization

- [x] Implement **warp to EPSG:3857** (or 4326) using GDAL (`gdal::programs::raster::warp` or equivalent stable API for your `gdal` version).
- [x] Expose **working dataset** handle + geotransform after warp.
- **Verify:** run on a small GeoTIFF; confirm bounds and pixel scale change as expected (log or debug assert).

### Phase 4 — Single tile read + resize + encode

- [x] Given `(z,x,y)`, compute **source window** in **source pixels** (reuse/refine `get_areas` logic with GDAL's affine).
- [x] Implement **`chunk_size`** builder setter on `GeoTiff`: the pipeline reads at most one chunk of source pixels into RAM at a time; tiles overlapping that chunk are processed before the buffer is dropped. Provide a default (e.g. 512 rows or a configurable byte cap) so callers that do not set it are safe on large inputs.
- [x] Read raster band(s) into chunk buffer via GDAL `RasterIO` windowed read; tiles within that chunk pull from the in-RAM buffer — no redundant GDAL reads per tile.
- [x] Resize chunk-extracted tile window to `tile_size` with `fast_image_resize` **or** GDAL `RasterIO` with appropriate resampling — **one** primary path.
- [x] Encode **PNG** via `image` (`encode` module).
- **Verify:** write one tile to `/tmp` and open in an image viewer; run with a deliberately tiny `chunk_size` (e.g. 1 row) and confirm output is identical to default `chunk_size` (no tile corruption at chunk boundaries).

### Phase 5 — Full pipeline + disk output

- [ ] Enumerate all tiles for `[min_z, max_z]` over dataset extent (with optional **crop bbox** args later).
- [ ] Implement `pipeline/chunks.rs`: outer loop iterates over **source-pixel chunks** (bounded by `GeoTiff::chunk_size`); inner loop processes all tiles whose source windows fall within the current chunk; buffer is released and next chunk read before moving on. This is the structure that keeps RAM bounded for 200 GB+ inputs.
- [ ] **Parallelize** the inner (per-tile) loop with `rayon` (`par_iter` over tiles within a chunk); the outer chunk loop remains sequential to bound peak memory. Use `tracing` spans to log chunk index, tile count per chunk, and elapsed time.
- [ ] Write tree `{z}/{x}/{y}.{ext}` for the selected default format (e.g. `.png`).
- **Verify:** run on a sample GeoTIFF with a small `chunk_size` to exercise multiple chunk iterations; confirm tile tree is complete and correct.

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
- [ ] **`pipeline`**: branch on `GeoTiff`'s `ResampleBackend` — CPU path unchanged; GPU path uses shared `(z,x,y)` + window math.
- [ ] **Verify:** `cargo build -p libgeotiles` (default, no GPU); `cargo build -p libgeotiles --features gpu` (or split features); manual or integration test on a machine with Vulkan or GLES; compare **visual** tile output to CPU for a few tiles (exact bytes may differ — document).

**Note:** CI without GPU can still **compile** `--all-features` if software Vulkan (e.g. lavapipe) or GLES is installed — follow **imgvwr’s** GPU matrix pattern in **§2.1**; local verification may be manual.

### Phase 8 — CLI binary + application config (**post–first-release**)

**Prerequisites:** **§9.1** (first library release) done; library API stable enough to wrap.

- [ ] Add **`geotiles`** crate to **`[workspace]`**; **`clap`**, **`tracing-subscriber`**, **`main`** wiring only — call into **`libgeotiles`**.
- [ ] **Config file format** and discovery — **design in this phase** (e.g. TOML, paths, merge order with env/CLI); document in-repo.
- [ ] Map argv + config → **`GeoTiff`** builder setters; exit codes, `--help`, optional completions feature if desired.
- [ ] Extend **§7.0** / CI to **two-crate** **`cargo`** matrices (**§2.1**).
- **Verify:** `cargo run -p geotiles -- --help` and end-to-end run against a real GeoTIFF.

---

## 8. Risk register

| Risk                                                      | Mitigation                                                                                                                                                                                                                                 |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| GDAL version mismatch on user machines                    | Document **supported GDAL** range; CI images pin distro packages per **§2.1** workflows                                                                                                                                                    |
| Large rasters exhaust RAM (200 GB+ inputs are real)       | `GeoTiff::chunk_size()` bounds the source-pixel buffer; outer loop reads one chunk, processes all overlapping tiles, frees buffer, then advances — full raster is never in RAM. A safe built-in default is mandatory (§4 step 4, Phase 4). |
| VRAM exhaustion on GPU path                               | Same `chunk_size` budget governs VRAM staging; GPU buffer freed before next chunk upload (§1.4, Phase 7).                                                                                                                                  |
| Double resample (warp + resize) blurs                     | Use GDAL with **appropriate overview** or single resample stage where possible                                                                                                                                                             |
| TMS/XYZ confusion                                         | One well-tested helper + explicit flag                                                                                                                                                                                                     |
| GPU **PCIe readback** cost negates wins                   | Profile; batch tiles; minimize readback size; document when GPU helps (large zoom jobs, many tiles)                                                                                                                                        |
| No Vulkan/GLES on host                                    | Library **falls back to CPU** or returns **error** per policy; future CLI may expose `--backend` (§5)                                                                                                                                      |
| AVIF/JXL **build complexity** (native deps, long compile) | Keep behind **features**; document **optional** packager deps; prefer **pure-Rust** where it meets quality/perf                                                                                                                            |
| **Network tests** flaky or slow                           | Do **not** download on every `cargo test`; use **§6** cache + checksums + `#[ignore]` / env gate                                                                                                                                           |
| **Fixture URL** moved or changed                          | Version the manifest; pin **SHA-256**; update URLs in one commit                                                                                                                                                                           |
| **`cargo-deny` / `typos` not installed** locally          | Document install (e.g. `cargo install cargo-deny typos-cli`); CI should run §7.0 when added                                                                                                                                                |

---

## 9. Definition of done

### 9.1 First release — **`libgeotiles` only** (§1.6)

- [ ] **§7.0 quality gates** pass for **`-p libgeotiles`** (license deny, typos, clippy both feature matrices, tests both matrices with `RUSTFLAGS='-D warnings'`).
- [ ] `cargo build --release -p libgeotiles` with default features.
- [ ] Library can produce a **valid** `{z}/{x}/{y}` tree (via API or **examples**/integration tests) from a **representative** GeoTIFF for at least **EPSG:4326** and **EPSG:3857** sources (after warp).
- [ ] **Tile crop** is performed entirely inside `libgeotiles` (no external tool invocation for the crop step).
- [ ] **Chunked I/O:** `GeoTiff::chunk_size()` setter present; pipeline never holds more than one chunk of source pixels in RAM; verified correct at chunk boundaries; safe default provided.
- [ ] **No** dependency on abandoned crates (per §3 policy).
- [ ] **No** requirement for Avalonia, Docker, or GTiff2Tiles.Console parity.
- [ ] **GPU path (Phase 7):** optional `gpu` features compile; CPU remains **default** features; optional manual/visual check vs CPU path for a few tiles.
- [ ] **Output formats (Phase 6):** at least **PNG** on default features; **JPEG**, **WebP**, **AVIF**, **JXL** via **optional** features with **documented** encoder choices and packager notes.
- [ ] **Testing (§6):** fixture manifest + cache helper; default `cargo test` **offline**; documented command for **ignored** / **fetch** integration tests; optional **synthetic** micro-GeoTIFF tests in-repo.
- [ ] **Benchmarks (§6.6):** `criterion` benchmark suite present; at least one CPU-path baseline result recorded; GPU vs CPU comparison run once GPU path (Phase 7) is available.
- [ ] **Logging:** `tracing` spans / events present throughout the pipeline (open → warp → tile loop → encode → write); no phase may be merged without its log points.
- [ ] **Repo automation (§2.1):** `.github/workflows`, Dependabot, `deny.toml`, `.typos.toml` adapted from tofi placeholder files (Phase 0) and aligned with **imgvwr** style (minimal renames + GDAL; **`-p libgeotiles`** in jobs until **`geotiles`** exists).

### 9.2 Post–first-release — **CLI + config** (Phase 8, §5)

- [ ] **`geotiles`** binary crate added; **§7.0** extended to **full workspace** (or **`-p geotiles`** where appropriate).
- [ ] **Application config** format chosen and documented (e.g. TOML); resolution order defined.
- [ ] End-to-end: **`geotiles`** produces tiles from argv + config on a real GeoTIFF.

---

## 10. Document maintenance

Update this file when: workspace layout changes, a phase is completed (checkboxes), dependency strategy changes, CRS/tile-default decisions, **release scope** (§1.6, §9), **CLI/config** (§5, §9.2), **testing/fixture policy** (§6), **quality gate commands** (§7.0), or **imgvwr template alignment** (§2.1) changes.

### Revision history

| Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-04-20 | Initial plan: core + CLI only; deps; phased steps; explicit non-goals                                                                                                                                                                                                                                                                                                                                                                                                      |
| 2026-04-20 | GPU tile crop/scale as optional migration target (`wgpu`); CPU default features + default CLI backend; Phase 8; §1.4 policy                                                                                                                                                                                                                                                                                                                                                |
| 2026-04-20 | Optional output formats: PNG, JPEG, WebP, AVIF, JXL (§1.5); `encode/` module; Phase 7 expanded; Cargo features                                                                                                                                                                                                                                                                                                                                                             |
| 2026-04-20 | §6 Testing strategy: real GeoTIFFs without LFS; cache + checksums; env override; not downloading every test; phased steps → §7                                                                                                                                                                                                                                                                                                                                             |
| 2026-04-20 | §7.0 Mandatory gates: `cargo deny`, `typos`, clippy `--all-features` / `--no-default-features`, `cargo test` both with `RUSTFLAGS='-D warnings'`                                                                                                                                                                                                                                                                                                                           |
| 2026-04-20 | §2.1: docs + CI/tooling organization mirrors [imgvwr](https://github.com/Gigas002/imgvwr) mostly unchanged; §1.2 non-goals adjusted; Phase 0 + §6.4 + DoD                                                                                                                                                                                                                                                                                                                  |
| 2026-04-20 | Library-first release (§1.6, §9.1); CLI + config deferred (§5, Phase 8, §9.2); workspace starts `libgeotiles`-only; §7.0 `-p libgeotiles`; phases renumbered                                                                                                                                                                                                                                                                                                               |
| 2026-04-21 | §1.1: performance+simplicity and logging-throughout as first-class goals; tile crop in crate stated as mandatory rationale; §1.2: docs.rs handles API docs, tofi placeholder files noted as already copied; §6.6: Benchmarks subsection added (`criterion`, CPU vs GPU, lib comparison); Phase 0: add step to adapt tofi placeholder files; Phase 1: logging-throughout requirement; §9.1 DoD: benchmark baseline criterion                                                |
| 2026-04-21 | §1.1: chunked/streaming I/O as first-class goal (200 GB+ inputs, configurable `chunk_size` on `GeoTiff`); §1.4 GPU work-split updated (VRAM chunk budget + free before next chunk); §2 layout: `pipeline/chunks.rs`; §3: `memmap2` note refined; §4: new step 4 (chunked read manager), renumbered subsequent steps; Phase 4/5: chunk-aware implementation steps; §6.6: `bench_chunk_size_sweep`; §8: RAM + VRAM exhaustion risks updated; §9.1 DoD: chunked I/O criterion |
| 2026-04-21 | §3: GeoRust ecosystem reference (georust.org) added; §6.0: mandatory test file architecture rule (no inline tests — unit tests in sibling `tests.rs`, integration tests in `tests/`) |
| 2026-04-21 | `TileJob` renamed to `GeoTiff` throughout; primary struct lives in `src/geotiff.rs`; API shape: `GeoTiff::open(path)?.zoom(..).chunk_size(..).format(..).output(..).crop()?`; `crop()` is the pipeline execution method; `ResampleBackend` and `TileFormat` unchanged; §1.3 parity row updated; §4 now opens with naming rationale + illustrative snippet; §2 layout updated (`geotiff.rs` added, `gdal_io/` marked internal)                                              |
| 2026-04-21 | Phase 4 complete: `source_window`, `read_chunk` in `gdal_io`; `crop_tile` + `ChunkBuffer` in `tile`; `encode` module (PNG/JPEG/WebP dispatch); `GeoTiff` builder with `crop()` (EPSG:4326 path); `TileFormat` variants no longer feature-gated; §7.0 gates pass both feature matrices |
