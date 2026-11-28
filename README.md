# geotiles-rs

A Rust library for generating map tiles from GeoTIFF sources.

Rewrite of my old [GTiff2Tiles](https://github.com/Gigas002/GTiff2Tiles) project, for diff see (docs/benchmarks.md).

---

## Input

`geotiles` cuts tiles; it does not convert rasters. The input must already be:

- a GeoTIFF with 8-bit unsigned samples, pixel-interleaved (not planar);
- gray, gray + alpha, RGB or RGBA (1–4 bands);
- north-up (no rotation), georeferenced by a pixel scale + single tie point or a
  transformation matrix;
- in EPSG:4326 or EPSG:3857 (taken from the GeoKeys; `--crs` is only needed when the file
  declares none).

Anything else is rejected with a message saying what to change. Do the preparation as a
separate step with whatever tool you like, e.g. with GDAL:

```sh
gdalwarp -t_srs EPSG:3857 -r lanczos in.tif warped.tif              # reproject
gdal_translate -ot Byte -scale in16.tif in8.tif                     # 16-bit → 8-bit
gdalwarp -dstalpha -srcnodata 0 in.tif rgba.tif                     # nodata → alpha
gdal_translate -expand rgb paletted.tif rgb.tif                     # palette → RGB
```

There are no system dependencies: decoding and every encoder are pure Rust (the `gpu`
feature needs a Vulkan/Metal/DX12/GL driver at runtime).

---

## Feature flags

Format features are **opt-in** except `png`. `geotiff` (GeoTIFF input) is on by default;
the library builds without it for callers with their own `RasterSource`.

| Feature      | What it enables                                 | Default |
| ------------ | ----------------------------------------------- | ------- |
| `png`        | PNG tile encoding                               | ✅      |
| `jpeg`       | JPEG tile encoding                              | ❌      |
| `webp`       | WebP tile encoding (lossless only for now)      | ❌      |
| `avif`       | AVIF tile encoding via `ravif` (pure Rust)      | ❌      |
| `jxl`        | JPEG XL encoding via `jxl-encoder` (pure Rust)  | ❌      |
| `geotiff`    | GeoTIFF input (`GeoTiff` raster source)         | ✅      |
| `gpu`        | GPU resample backend (`wgpu`)                   | ❌      |

---
