# geotiles-rs

A Rust library for generating map tiles from GeoTIFF sources.

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
| `gpu`        | GPU tile pipeline                               | ❌      |

---
