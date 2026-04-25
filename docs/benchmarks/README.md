# geotiles-rs benchmark baselines

Snapshot results for the `criterion`-based benchmark suite in
`libgeotiles/benches/`. Re-record whenever the hot path (resample, encode,
pipeline parallelism, GPU context) changes — see §6.6 of the migration plan.

---

## How to run

```sh
# CPU path (default features):
cargo bench -p libgeotiles

# All encode formats:
cargo bench -p libgeotiles --bench encode --features png,jpeg,webp,avif,jxl

# GPU path (requires Vulkan / GLES or software renderer such as lavapipe):
cargo bench -p libgeotiles --bench pipeline --features gpu
```

Criterion writes an HTML report to `target/criterion/`. Copy the numbers you
want to preserve into the tables below.

---

## Recording format

| Benchmark                                           | Machine   | Rust        | Features | Mean (ms) | Std dev | Date       |
| --------------------------------------------------- | --------- | ----------- | -------- | --------- | ------- | ---------- |
| _(example)_ `tile_resample_cpu/rgba_256x256_to_256` | i7-12700K | 1.87 stable | default  | 0.123     | ±0.002  | 2026-04-25 |

Add one row per significant measurement. "Machine" should be enough to
reproduce hardware context (CPU model or GPU model for GPU rows).

---

## CPU resample baselines (`bench_tile_resample_cpu`)

_No baselines recorded yet. Run `cargo bench -p libgeotiles --bench pipeline`
on a stable machine and paste the Criterion summary here._

| Benchmark | Machine | Rust | Features | Mean (ms) | Std dev | Date |
| --------- | ------- | ---- | -------- | --------- | ------- | ---- |

---

## GPU resample baselines (`bench_tile_resample_gpu`)

_No baselines recorded yet. Run
`cargo bench -p libgeotiles --bench pipeline --features gpu`
on a machine with Vulkan or GLES support and paste results here._

Baseline comparisons should cover the same input sizes as the CPU table so
the crossover point (tile count / zoom level at which GPU overhead pays off)
can be identified.

| Benchmark | Machine / GPU | Rust | Features | Mean (ms) | Std dev | CPU mean (ms) | Speedup | Date |
| --------- | ------------- | ---- | -------- | --------- | ------- | ------------- | ------- | ---- |

---

## Encode baselines (`bench_tile_encode_*`)

_No baselines recorded yet. Run
`cargo bench -p libgeotiles --bench encode --features png,jpeg,webp,avif,jxl`
and paste results here._

| Benchmark | Format | Machine | Rust | Features | Mean (ms) | Std dev | Date |
| --------- | ------ | ------- | ---- | -------- | --------- | ------- | ---- |

---

## Chunk-size sweep (`bench_chunk_size_sweep`)

_Not yet implemented (planned for Phase 5 follow-up, §6.6)._

Once added, record results for chunk sizes 64 / 256 / 512 / 1 024 / 4 096
rows on a fixed representative GeoTIFF to identify the RAM-vs-throughput
sweet spot.

| chunk_size (rows) | Machine | Input raster | Mean total (s) | Peak RSS (MB) | Date |
| ----------------- | ------- | ------------ | -------------- | ------------- | ---- |

---

## Notes

- All wall-time means are Criterion's **lower-bound** estimate (`mean` from
  the HTML report), not the raw minimum.
- Re-run before **and** after any change to the resample, encode, or pipeline
  parallelism code and record both rows so regressions are visible.
- GPU vs CPU comparison: document the crossover point once Phase 7 data is
  available (see §7, §9.1 of the migration plan).
