#!/usr/bin/env bash
# geotiles (CPU/GPU) vs GTiff2Tiles.Console benchmark suite.
#
# Self-contained: on a machine that has none of this set up, running this script
# will (best-effort) install the missing toolchains, clone GTiff2Tiles next to
# this repo, download and prepare the "big" test raster, build both projects in
# release mode, and run the full benchmark matrix documented in this directory's
# benchmarks.md (small/big datasets x png/jxl formats x geotiles-cpu/geotiles-gpu/
# gtiff2tiles).
#
# Usage:
#   ./bench.sh                  # full run: setup + build + fetch data + benchmark
#   ./bench.sh --skip-setup     # assume toolchains/repos/data/builds are ready; just benchmark
#   ./bench.sh --setup-only     # only do setup (install/clone/fetch/build), don't benchmark
#
# Tuning via environment variables (defaults shown):
#   RUNS=3                        repetitions per (dataset, format, tool) cell
#   SMALL_ZOOM=0..18               geotiles --zoom for the small dataset
#   SMALL_MINZ=0  SMALL_MAXZ=18    GTiff2Tiles.Console --minz/--maxz equivalents
#   BIG_ZOOM=1..7
#   BIG_MINZ=1    BIG_MAXZ=7
#   FORMATS="png jxl"              space-separated; GTiff2Tiles only gets a row for png
#                                   (it has no JXL support)
#   GTIFF2TILES_DIR=<auto>         path to a GTiff2Tiles checkout; default: a
#                                   sibling directory of this repo, cloned if missing
#   BENCH_DATA_DIR=<auto>          where the big-file test raster is downloaded/cached;
#                                   default: <workspace>/.geotiles_bench_data
#   RESULTS_CSV=<auto>             output CSV path; default: ./bench-results-<date>.csv
#     (relative to this script's directory)
#
# Requires (auto-installed where possible): rustc/cargo (via rustup), a .NET SDK
# (via dotnet-install.sh), gdal_translate (via the system package manager), git,
# curl, unzip. GPU rows require a Vulkan/Metal/DX12-capable GPU; if `geotiles
# --backend gpu` can't initialise one, GPU rows are skipped automatically with a
# warning rather than failing the whole run.
#
# Tested on Arch/CachyOS (pacman); should work on Debian/Ubuntu (apt-get) and
# Fedora (dnf) too. macOS should mostly work (brew) except GTiff2Tiles.Console's
# NuGet-bundled native deps are Windows/Linux only — GTiff2Tiles rows will fail
# there; geotiles CPU/GPU rows are pure Rust and should still work. Windows is
# not supported by this script directly (use WSL).

# Deliberately not `set -e`: this script runs unattended for tens of minutes
# across many (dataset, format, tool) cells, and one cell's hiccup (a transient
# GPU error, a metrics-collection edge case, etc.) shouldn't abort the whole
# run. Critical steps (toolchain setup, builds, data prep) check their own
# exit status explicitly via `die` instead. `run_one` always returns a status
# and logs failures, but callers don't propagate it — failed cells show up as
# `FAILED`/`tile_count=0` rows in the CSV rather than killing the script.
set -uo pipefail

# ── Paths ───────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GEOTILES_RS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_DIR="$(cd "$GEOTILES_RS_DIR/.." && pwd)"

GTIFF2TILES_DIR="${GTIFF2TILES_DIR:-$WORKSPACE_DIR/GTiff2Tiles}"
BENCH_DATA_DIR="${BENCH_DATA_DIR:-$WORKSPACE_DIR/.geotiles_bench_data}"
RESULTS_CSV="${RESULTS_CSV:-$SCRIPT_DIR/bench-results-$(date +%Y%m%d-%H%M%S).csv}"

GEOTILES_BIN="$GEOTILES_RS_DIR/target/release/geotiles"
G2T_BIN="$GTIFF2TILES_DIR/artifacts/bin/GTiff2Tiles.Console/release/GTiff2Tiles.Console"
SMALL_INPUT="$GTIFF2TILES_DIR/Examples/Input/Input4326.tif"
BIG_INPUT="$BENCH_DATA_DIR/HYP_HR_SR_W_crop.tif"

RUNS="${RUNS:-3}"
SMALL_ZOOM="${SMALL_ZOOM:-0..18}"
SMALL_MINZ="${SMALL_MINZ:-0}"
SMALL_MAXZ="${SMALL_MAXZ:-18}"
BIG_ZOOM="${BIG_ZOOM:-1..7}"
BIG_MINZ="${BIG_MINZ:-1}"
BIG_MAXZ="${BIG_MAXZ:-7}"
FORMATS="${FORMATS:-png jxl}"

SKIP_SETUP=0
SETUP_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --skip-setup) SKIP_SETUP=1 ;;
        --setup-only) SETUP_ONLY=1 ;;
        -h|--help) sed -n '2,40p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "Unknown argument: $arg" >&2; exit 1 ;;
    esac
done

# ── Helpers ─────────────────────────────────────────────────────────────────

log() { printf '[%s] %s\n' "$(date '+%H:%M:%S')" "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

PKG_MANAGER=""
detect_pkg_manager() {
    if have pacman; then PKG_MANAGER=pacman
    elif have apt-get; then PKG_MANAGER=apt-get
    elif have dnf; then PKG_MANAGER=dnf
    elif have brew; then PKG_MANAGER=brew
    else PKG_MANAGER=""
    fi
}

# Installs logical package names, translated per detected package manager.
install_pkgs() {
    [ -n "$PKG_MANAGER" ] || detect_pkg_manager
    local pacman_pkgs=() apt_pkgs=() dnf_pkgs=() brew_pkgs=()
    for p in "$@"; do
        case "$p" in
            gdal) pacman_pkgs+=(gdal); apt_pkgs+=(gdal-bin); dnf_pkgs+=(gdal); brew_pkgs+=(gdal) ;;
            git) pacman_pkgs+=(git); apt_pkgs+=(git); dnf_pkgs+=(git); brew_pkgs+=(git) ;;
            curl) pacman_pkgs+=(curl); apt_pkgs+=(curl); dnf_pkgs+=(curl); brew_pkgs+=(curl) ;;
            unzip) pacman_pkgs+=(unzip); apt_pkgs+=(unzip); dnf_pkgs+=(unzip); brew_pkgs+=(unzip) ;;
            build-tools) pacman_pkgs+=(base-devel); apt_pkgs+=(build-essential); dnf_pkgs+=("@development-tools") ;;
        esac
    done
    case "$PKG_MANAGER" in
        pacman) sudo pacman -Sy --needed --noconfirm "${pacman_pkgs[@]}" ;;
        apt-get) sudo apt-get update && sudo apt-get install -y "${apt_pkgs[@]}" ;;
        dnf) sudo dnf install -y "${dnf_pkgs[@]}" ;;
        brew) brew install "${brew_pkgs[@]}" ;;
        *) die "No supported package manager found (pacman/apt-get/dnf/brew). Install manually: $*" ;;
    esac
}

# ── Toolchain setup ─────────────────────────────────────────────────────────

ensure_rust() {
    if have cargo && have rustc; then
        log "Rust toolchain found: $(rustc --version)"
        return
    fi
    log "Rust toolchain not found — installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    have cargo || die "rustup install completed but cargo still not on PATH"
}

ensure_dotnet() {
    if have dotnet; then
        log ".NET SDK found: $(dotnet --version)"
        return
    fi
    log ".NET SDK not found — installing via dotnet-install.sh (channel 10.0)..."
    curl -sSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh
    bash /tmp/dotnet-install.sh --channel 10.0 --install-dir "$HOME/.dotnet"
    export PATH="$HOME/.dotnet:$PATH"
    have dotnet || die "dotnet-install.sh completed but dotnet still not on PATH (try: export PATH=\"\$HOME/.dotnet:\$PATH\")"
}

ensure_gdal() {
    if have gdal_translate && have gdalinfo; then
        log "GDAL CLI found: $(gdalinfo --version)"
        return
    fi
    log "gdal_translate not found — installing GDAL..."
    install_pkgs gdal
    have gdal_translate || die "GDAL install attempted but gdal_translate still missing; install it manually"
}

ensure_basic_tools() {
    local missing=()
    for t in git curl unzip; do have "$t" || missing+=("$t"); done
    [ ${#missing[@]} -eq 0 ] && return
    log "Installing missing basic tools: ${missing[*]}"
    install_pkgs "${missing[@]}"
}

ensure_gtiff2tiles_repo() {
    if [ -d "$GTIFF2TILES_DIR/.git" ]; then
        log "GTiff2Tiles checkout found at $GTIFF2TILES_DIR"
        return
    fi
    log "Cloning GTiff2Tiles into $GTIFF2TILES_DIR..."
    git clone --depth 1 https://github.com/Gigas002/GTiff2Tiles.git "$GTIFF2TILES_DIR" \
        || die "Failed to clone GTiff2Tiles into $GTIFF2TILES_DIR"
}

build_geotiles() {
    log "Building geotiles-rs (release, all-features)..."
    (cd "$GEOTILES_RS_DIR" && cargo build --release --all-features -p geotiles)
    [ -x "$GEOTILES_BIN" ] || die "geotiles binary not found at $GEOTILES_BIN after build"
}

build_gtiff2tiles() {
    log "Building GTiff2Tiles.Console (Release)..."
    (cd "$GTIFF2TILES_DIR" && dotnet build -c Release GTiff2Tiles.Console/GTiff2Tiles.Console.csproj)
    [ -x "$G2T_BIN" ] || die "GTiff2Tiles.Console binary not found at $G2T_BIN after build"
}

fetch_small_input() {
    [ -f "$SMALL_INPUT" ] || die "Small test input not found at $SMALL_INPUT (expected bundled in the GTiff2Tiles repo)"
    log "Small input ready: $SMALL_INPUT"
}

fetch_big_input() {
    if [ -f "$BIG_INPUT" ]; then
        log "Big input already prepared: $BIG_INPUT"
        return
    fi
    mkdir -p "$BENCH_DATA_DIR"
    local raw="$BENCH_DATA_DIR/HYP_HR_SR_W.tif"
    local zip="$BENCH_DATA_DIR/HYP_HR_SR_W.zip"
    if [ ! -f "$raw" ]; then
        log "Downloading Natural Earth hypsometric raster (~380 MB)..."
        curl -L --fail --retry 3 --retry-delay 5 -o "$zip" \
            "https://naciscdn.org/naturalearth/10m/raster/HYP_HR_SR_W.zip"
        (cd "$BENCH_DATA_DIR" && unzip -o "$(basename "$zip")" HYP_HR_SR_W.tif)
        rm -f "$zip"
    fi
    # GTiff2Tiles.Console throws on rasters with *exact* global bounds
    # (-180/180/-90/90) — crop a small margin off to sidestep that.
    log "Cropping a margin off the raster (avoids a GTiff2Tiles bounds-exact crash)..."
    gdal_translate -srcwin 10 10 21580 10780 -co COMPRESS=NONE "$raw" "$BIG_INPUT"
    rm -f "$raw"
    [ -f "$BIG_INPUT" ] || die "Failed to prepare big input at $BIG_INPUT"
}

check_gpu_available() {
    local tmp; tmp=$(mktemp -d)
    if "$GEOTILES_BIN" --input "$SMALL_INPUT" --output "$tmp" --zoom 0..0 \
        --extension png --tms true --crs geographic --bands 4 --tilesize 256 \
        --backend gpu >/dev/null 2>&1; then
        rm -rf "$tmp"
        return 0
    fi
    rm -rf "$tmp"
    return 1
}

run_setup() {
    ensure_basic_tools
    ensure_rust
    ensure_dotnet
    ensure_gdal
    ensure_gtiff2tiles_repo
    build_geotiles
    build_gtiff2tiles
    fetch_small_input
    fetch_big_input
    log "Setup complete."
}

# ── Benchmark harness ───────────────────────────────────────────────────────
# Measures wall time (via `time`), peak RSS (via /proc/<pid>/status polling),
# and — for geotiles GPU runs, when nvidia-smi is available — GPU utilization
# and VRAM (via nvidia-smi polling). Mirrors the methodology described in
# benchmarks.md.

poll_rss() {
    local match="$1" outfile="$2" pid="" max_kb=0 tries=0
    while [ -z "$pid" ] && [ "$tries" -lt 1000 ]; do
        pid=$(pgrep -f -n -- "$match" 2>/dev/null || true)
        tries=$((tries + 1))
        [ -z "$pid" ] && sleep 0.05
    done
    if [ -z "$pid" ]; then
        echo 0 > "$outfile"
        return
    fi
    while kill -0 "$pid" 2>/dev/null; do
        kb=$(awk '/VmRSS/{print $2}' "/proc/$pid/status" 2>/dev/null || true)
        if [ -n "${kb:-}" ] && [ "$kb" -gt "$max_kb" ] 2>/dev/null; then
            max_kb=$kb
        fi
        sleep 0.1
    done
    echo "$max_kb" > "$outfile"
}

HAVE_NVIDIA_SMI=0
have nvidia-smi && HAVE_NVIDIA_SMI=1

# run_one <dataset> <format> <tool> <run_idx> <outdir> <unique_match_substring> <poll_gpu:0|1> -- <command...>
run_one() {
    local dataset="$1" format="$2" tool="$3" run_idx="$4" outdir="$5" match="$6" poll_gpu="$7"
    shift 7
    rm -rf "$outdir"
    mkdir -p "$outdir"
    local logdir; logdir=$(mktemp -d)
    local rss_file; rss_file=$(mktemp)
    poll_rss "$match" "$rss_file" &
    local poller_pid=$!

    local gpu_file="" gpu_pid=""
    if [ "$poll_gpu" = "1" ] && [ "$HAVE_NVIDIA_SMI" = "1" ]; then
        gpu_file=$(mktemp)
        nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader,nounits -lms 200 \
            > "$gpu_file" 2>/dev/null &
        gpu_pid=$!
    fi

    local timing_file; timing_file=$(mktemp)
    TIMEFORMAT='%R %U %S'
    { time "$@" >"$logdir/stdout.log" 2>"$logdir/stderr.log"; } 2>"$timing_file"
    local status=$?

    [ -n "$gpu_pid" ] && kill "$gpu_pid" 2>/dev/null || true
    wait "$poller_pid" 2>/dev/null || true

    local max_kb max_mb
    max_kb=$(cat "$rss_file" 2>/dev/null || echo 0)
    max_mb=$(awk -v k="$max_kb" 'BEGIN{printf "%.1f", k/1024}')
    local real_s user_s sys_s
    read -r real_s user_s sys_s < "$timing_file"
    local avg_cores
    avg_cores=$(awk -v u="$user_s" -v s="$sys_s" -v r="$real_s" 'BEGIN{ if (r+0>0) printf "%.2f", (u+s)/r; else print "0.00" }')

    local gpu_avg="" gpu_max="" gpu_vram=""
    if [ -n "$gpu_file" ]; then
        read -r gpu_avg gpu_max gpu_vram < <(awk -F',' '
            { gsub(/ /,"",$1); gsub(/ /,"",$2);
              if ($1 ~ /^[0-9]+$/) { u=$1+0; sum+=u; n++; if (u>maxu) maxu=u }
              if ($2 ~ /^[0-9]+$/) { m=$2+0; if (m>maxm) maxm=m } }
            END { if (n>0) printf "%.1f %d %d\n", sum/n, maxu, maxm; else print "0 0 0" }
        ' "$gpu_file")
    fi

    local tile_count total_bytes
    tile_count=$(find "$outdir" -type f | wc -l)
    total_bytes=$(du -sb "$outdir" 2>/dev/null | cut -f1)

    if [ "$status" -ne 0 ] || [ "$tile_count" -eq 0 ]; then
        log "$dataset/$format/$tool run$run_idx FAILED (status=$status): $(tail -5 "$logdir/stderr.log" 2>/dev/null)"
    else
        log "$dataset/$format/$tool run$run_idx: real=${real_s}s tiles=$tile_count bytes=$total_bytes rss=${max_mb}MB"
    fi
    echo "$dataset,$format,$tool,$run_idx,$real_s,$user_s,$sys_s,$avg_cores,$max_mb,$gpu_avg,$gpu_max,$gpu_vram,${tile_count},${total_bytes}" >> "$RESULTS_CSV"

    rm -rf "$rss_file" "$timing_file" "$logdir" "$outdir"
    [ -n "$gpu_file" ] && rm -f "$gpu_file"
    return $status
}

bench_dataset() {
    local dataset="$1" input="$2" zoom_geotiles="$3" minz="$4" maxz="$5"
    local work; work=$(mktemp -d)

    for format in $FORMATS; do
        log "=== [$dataset/$format] geotiles CPU ==="
        for i in $(seq 1 "$RUNS"); do
            run_one "$dataset" "$format" "geotiles-cpu" "$i" \
                "$work/${dataset}_${format}_cpu_run${i}" "${dataset}_${format}_cpu_run${i}" "0" \
                "$GEOTILES_BIN" --input "$input" --output "$work/${dataset}_${format}_cpu_run${i}" \
                --zoom "$zoom_geotiles" --extension "$format" --tms true --crs geographic --bands 4 \
                --tilesize 256 --chunk-size 4096 --backend cpu
        done

        if [ "$GPU_AVAILABLE" = "1" ]; then
            log "=== [$dataset/$format] geotiles GPU ==="
            for i in $(seq 1 "$RUNS"); do
                run_one "$dataset" "$format" "geotiles-gpu" "$i" \
                    "$work/${dataset}_${format}_gpu_run${i}" "${dataset}_${format}_gpu_run${i}" "1" \
                    "$GEOTILES_BIN" --input "$input" --output "$work/${dataset}_${format}_gpu_run${i}" \
                    --zoom "$zoom_geotiles" --extension "$format" --tms true --crs geographic --bands 4 \
                    --tilesize 256 --chunk-size 4096 --backend gpu
            done
        else
            log "=== [$dataset/$format] geotiles GPU — skipped, no usable GPU backend found ==="
        fi

        if [ "$format" = "png" ]; then
            log "=== [$dataset/$format] GTiff2Tiles.Console ==="
            for i in $(seq 1 "$RUNS"); do
                local tmpdir="$work/${dataset}_g2t_tmp_run${i}"
                mkdir -p "$tmpdir"
                run_one "$dataset" "$format" "gtiff2tiles" "$i" \
                    "$work/${dataset}_${format}_g2t_run${i}" "${dataset}_${format}_g2t_run${i}" "0" \
                    "$G2T_BIN" -i "$input" -o "$work/${dataset}_${format}_g2t_run${i}" \
                    --minz "$minz" --maxz "$maxz" -e png -t "$tmpdir" --tms true -c geodetic \
                    --interpolation lanczos3 -b 4 --tilecache 4000 --memcache 4294967296 \
                    -p false --timeleft false --tilesize 256 --tmr false --threads 0
                rm -rf "$tmpdir"
            done
        fi
    done

    rmdir "$work" 2>/dev/null || true
}

print_summary() {
    log "=== Summary (mean over $RUNS runs) ==="
    awk -F',' '
        NR==1 { next }
        {
            key = $1","$2","$3
            real[key] += $5; n[key]++
            tiles[key] = $13
            bytes[key] = $14
        }
        END {
            printf "%-8s %-6s %-14s %10s %8s %12s\n", "dataset", "format", "tool", "mean_real_s", "tiles", "total_MB"
            for (k in real) {
                split(k, parts, ",")
                printf "%-8s %-6s %-14s %10.2f %8d %12.1f\n", parts[1], parts[2], parts[3], real[k]/n[k], tiles[k], bytes[k]/1024/1024
            }
        }
    ' "$RESULTS_CSV" | sort >&2
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
    log "geotiles-rs dir: $GEOTILES_RS_DIR"
    log "GTiff2Tiles dir: $GTIFF2TILES_DIR"
    log "Bench data dir:  $BENCH_DATA_DIR"
    log "Results CSV:     $RESULTS_CSV"

    if [ "$SKIP_SETUP" = "0" ]; then
        run_setup
    else
        log "Skipping setup (--skip-setup); assuming toolchains/repos/data/builds are ready."
        [ -x "$GEOTILES_BIN" ] || die "$GEOTILES_BIN not found — run without --skip-setup first"
        [ -x "$G2T_BIN" ] || die "$G2T_BIN not found — run without --skip-setup first"
        [ -f "$SMALL_INPUT" ] || die "$SMALL_INPUT not found"
        [ -f "$BIG_INPUT" ] || die "$BIG_INPUT not found"
    fi

    [ "$SETUP_ONLY" = "1" ] && { log "Setup-only requested, stopping here."; exit 0; }

    log "Checking GPU backend availability..."
    GPU_AVAILABLE=0
    if check_gpu_available; then
        GPU_AVAILABLE=1
        log "GPU backend available — GPU rows will run."
    else
        log "GPU backend not available (no Vulkan/Metal/DX12 adapter, or wgpu init failed) — GPU rows will be skipped."
    fi

    echo "dataset,format,tool,run,real_s,user_s,sys_s,avg_cores,max_rss_mb,gpu_avg_pct,gpu_max_pct,gpu_max_vram_mb,tile_count,total_bytes" > "$RESULTS_CSV"

    bench_dataset "small" "$SMALL_INPUT" "$SMALL_ZOOM" "$SMALL_MINZ" "$SMALL_MAXZ"
    bench_dataset "big" "$BIG_INPUT" "$BIG_ZOOM" "$BIG_MINZ" "$BIG_MAXZ"

    print_summary
    log "Done. Raw results: $RESULTS_CSV"
}

main
