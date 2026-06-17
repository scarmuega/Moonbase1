#!/usr/bin/env bash
#
# fetch_data.sh — fetch the It-0 Shackleton source rasters into data/raw/.
#
# These are the raw inputs the offline `geo_pipeline` bake reprojects + tiles; the game
# never loads them. See data/README.md for provenance and the full pipeline.
#
# What it produces:
#   * DEM (always): PGDA LOLA 5 m/px Site04 (Shackleton rim) surface elevation GeoTIFF
#     — ~41 MB, downloaded whole. Saved as a name `geo_pipeline` finds (contains "ldem").
#   * Imagery (--with-imagery): the south-pole crop of the LROC NAC South Pole PSR mosaic
#     (~11.7 m/px, already polar-stereographic, contrast-stretched). The full mosaic is
#     ~2.7 GB, but GDAL's /vsicurl reads ONLY the bytes covering the DEM extent — a few
#     tens of MB, a few seconds. Requires GDAL (`brew install gdal`).
#
#     Why NAC over the older WAC global 100 m mosaic: the WAC global product is
#     equirectangular and degenerates at the pole (orbital seams + NoData coverage gaps);
#     the NAC South Pole mosaic is a controlled polar-stereographic mosaic ~10x finer with
#     no seams. To go back to WAC (or any other source), pass --imagery-url.
#
# Usage:
#   scripts/fetch_data.sh                       # DEM only (recommended first run)
#   scripts/fetch_data.sh --with-imagery        # DEM + the NAC polar crop (small, fast)
#   scripts/fetch_data.sh --imagery-url <URL>   # crop from a different remote mosaic
#   scripts/fetch_data.sh --crop-mpp <m>        # crop resolution (default 10 m/px)
#   scripts/fetch_data.sh --raw <dir>           # override the output dir (default data/raw)
#
#   # Regional "southpole" surface — a large, pole-wide flyover (site=southpole):
#   scripts/fetch_data.sh --region             # 120 km @ 40 m/px crop + imagery → data/raw_southpole
#   scripts/fetch_data.sh --region-km <km>     # crop side in km (default 120)
#   scripts/fetch_data.sh --region-mpp <m>     # DEM m/px (default 40 — the reliable overview)
#   scripts/fetch_data.sh --site <name>        # output prefix + bake --site (default shackleton)
#
# The DEM download resumes on re-run (curl -C -) and is skipped when already complete.

set -euo pipefail

# --- Sources (see data/README.md for provenance) ---------------------------
DEM_URL="https://pgda.gsfc.nasa.gov/data/LOLA_5mpp/Site04/Site04_final_adj_5mpp_surf.tif"
DEM_OUT="shackleton_ldem_5mpp.tif"

# Imagery source: LROC NAC South Pole PSR mosaic (contrast-stretched), polar stereographic,
# ~11.7 m/px, NoData=0. Direct GeoTIFF on the LROC PDS node (vsicurl reads byte ranges).
# NASA/GSFC/Arizona State University. (Older WAC global 100 m mosaic kept below for ref.)
IMAGERY_URL_DEFAULT="https://pds.lroc.im-ldi.com/data/LRO-L-LROC-5-RDR-V1.0/LROLRC_2001/EXTRAS/BROWSE/NAC_POLE/NAC_POLE_PSR_SOUTH/NAC_POLE_PSR_SOUTH_STRETCH.TIF"
# WAC fallback: https://asc-pds-services.s3.us-west-2.amazonaws.com/mosaic/Lunar_LRO_LROC-WAC_Mosaic_global_100m_June2013.tif
IMAGERY_OUT="shackleton_nac_psr_mosaic.tif"
# Crop resolution + padding around the DEM extent. The NAC source is ~11.7 m/px, so 10 m/px
# preserves its detail; the bake re-warps this onto the exact 5 m/px DEM grid anyway.
CROP_MPP=10
CROP_PAD_M=500

# --- Regional mode (--region): a large, pole-wide flyover surface ------------
# Crops a big square around the south pole from the LOLA 20 m/px 80S polar LDEM
# (PGDA "A New View of the Lunar South Pole", product 90). That COG throttles
# hard and its FULL-RES tile index is flaky over /vsicurl — but reading a coarser
# OVERVIEW (`-ovr AUTO` at REGION_MPP) needs far fewer, smaller index reads and is
# reliable + fast. The DEM is pole-centered (x_0=y_0=0), so the crop is symmetric
# about (0,0); imagery follows the DEM extent via crop_imagery() as usual.
REGION_DEM_URL="https://pgda.gsfc.nasa.gov/data/LOLA_20mpp/LDEM_80S_20MPP_ADJ.TIF"
REGION_EXTENT_KM=120     # side of the square crop (km), centered on the pole
REGION_MPP=40            # output DEM m/px (40 m = the LDEM's 1st overview: reliable)
MODE=site                # `site` (fixed Site04 DEM) or `region` (--region)
SITE=shackleton          # output filename prefix + the --site to pass to the bake

# --- Resolve paths relative to the repo root (script lives in scripts/). ----
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RAW_DIR="data/raw"
WANT_IMAGERY=0
IMAGERY_URL="$IMAGERY_URL_DEFAULT"

usage() { sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//; $d'; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-imagery) WANT_IMAGERY=1; shift ;;
        --imagery-url) IMAGERY_URL="${2:?--imagery-url needs a URL}"; WANT_IMAGERY=1; shift 2 ;;
        --crop-mpp) CROP_MPP="${2:?--crop-mpp needs a value}"; shift 2 ;;
        --raw) RAW_DIR="${2:?--raw needs a dir}"; RAW_SET=1; shift 2 ;;
        --region) MODE=region; SITE=southpole; WANT_IMAGERY=1; shift ;;
        --region-km) REGION_EXTENT_KM="${2:?--region-km needs a value}"; MODE=region; SITE=southpole; WANT_IMAGERY=1; shift 2 ;;
        --region-mpp) REGION_MPP="${2:?--region-mpp needs a value}"; MODE=region; SITE=southpole; WANT_IMAGERY=1; shift 2 ;;
        --site) SITE="${2:?--site needs a name}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "error: unexpected argument '$1'" >&2; usage >&2; exit 2 ;;
    esac
done

# Region mode names its rasters per-site and defaults to a sibling raw dir so it
# never collides with the fixed Site04 bake in data/raw (the bake auto-discovers a
# DEM by keyword across the whole raw dir, so one DEM per dir).
if [[ "$MODE" == region ]]; then
    DEM_OUT="${SITE}_ldem.tif"
    IMAGERY_OUT="${SITE}_nac_psr_mosaic.tif"
    CROP_MPP="$REGION_MPP"
    [[ "${RAW_SET:-0}" == 1 ]] || RAW_DIR="data/raw_${SITE}"
fi

# Make RAW_DIR absolute (relative paths resolve against the repo root, not $PWD).
case "$RAW_DIR" in /*) : ;; *) RAW_DIR="$REPO_ROOT/$RAW_DIR" ;; esac
mkdir -p "$RAW_DIR"

# Return the remote Content-Length (following redirects), or empty if unknown.
remote_size() {
    # tolower() for portability: macOS ships BSD awk, which ignores IGNORECASE.
    curl -sIL --max-time 60 "$1" 2>/dev/null \
        | awk 'tolower($0) ~ /^content-length:/ {n=$2} END {gsub(/\r/,"",n); print n}'
}
local_size() { [[ -f "$1" ]] && wc -c <"$1" | tr -d ' ' || echo 0; }

# Download $1 -> $RAW_DIR/$2, resuming; skip if already byte-complete.
fetch() {
    local url="$1" out="$RAW_DIR/$2" label="$3" want have
    want="$(remote_size "$url")"
    have="$(local_size "$out")"
    if [[ -n "$want" && "$have" == "$want" ]]; then
        echo "✓ $label already complete ($have bytes): $out"
        return 0
    fi
    echo "↓ $label"
    echo "  from $url"
    echo "  into $out${want:+  (~$want bytes)}"
    curl -fL --retry 3 --retry-delay 5 -C - -o "$out" "$url"
    echo "✓ $label done: $out ($(local_size "$out") bytes)"
}

# Crop the south-pole region out of a (remote) global mosaic, registered to the DEM grid.
crop_imagery() {
    local url="$1" out="$RAW_DIR/$IMAGERY_OUT" dem="$RAW_DIR/$DEM_OUT"
    for t in gdalwarp gdalinfo gdalsrsinfo; do
        command -v "$t" >/dev/null || { echo "error: '$t' not found — run 'brew install gdal'" >&2; exit 1; }
    done
    command -v python3 >/dev/null || { echo "error: python3 not found (needed to read DEM extent)" >&2; exit 1; }
    [[ -f "$dem" ]] || { echo "error: DEM missing at $dem" >&2; exit 1; }

    # Target grid = the DEM's CRS + extent (padded), so the imagery registers exactly.
    local proj4 minx miny maxx maxy
    proj4="$(gdalsrsinfo -o proj4 "$dem" | grep -m1 '+proj')"
    read -r minx miny maxx maxy < <(gdalinfo -json "$dem" \
        | python3 -c "import json,sys;c=json.load(sys.stdin)['cornerCoordinates'];print(c['lowerLeft'][0],c['lowerLeft'][1],c['upperRight'][0],c['upperRight'][1])")
    minx=$(python3 -c "print($minx-$CROP_PAD_M)"); miny=$(python3 -c "print($miny-$CROP_PAD_M)")
    maxx=$(python3 -c "print($maxx+$CROP_PAD_M)"); maxy=$(python3 -c "print($maxy+$CROP_PAD_M)")

    echo "↓ Imagery (polar crop via /vsicurl — reads only the south-pole bytes)"
    echo "  from $url"
    echo "  grid $minx $miny $maxx $maxy @ ${CROP_MPP} m/px"
    GDAL_DISABLE_READDIR_ON_OPEN=YES CPL_VSIL_CURL_ALLOWED_EXTENSIONS=.tif,.TIF \
    GDAL_HTTP_MULTIRANGE=YES GDAL_HTTP_MERGE_CONSECUTIVE_RANGES=YES \
        gdalwarp -overwrite -q \
        -t_srs "$proj4" -te "$minx" "$miny" "$maxx" "$maxy" -tr "$CROP_MPP" "$CROP_MPP" \
        -r bilinear "/vsicurl/$url" "$out"
    echo "✓ Imagery done: $out ($(local_size "$out") bytes)"
}

# Crop a large pole-centered square from the regional LOLA LDEM via /vsicurl, reading
# a coarse overview (reliable where the full-res tile index is not). See REGION_* notes.
crop_region_dem() {
    local out="$RAW_DIR/$DEM_OUT" half
    command -v gdalwarp >/dev/null || { echo "error: 'gdalwarp' not found — run 'brew install gdal'" >&2; exit 1; }
    command -v python3 >/dev/null || { echo "error: python3 not found" >&2; exit 1; }
    half=$(python3 -c "print(int($REGION_EXTENT_KM*1000/2))")
    echo "↓ Regional DEM (pole-centered crop via /vsicurl, ${REGION_MPP} m overview)"
    echo "  from $REGION_DEM_URL"
    echo "  box ±${half} m @ ${REGION_MPP} m/px"
    GDAL_DISABLE_READDIR_ON_OPEN=EMPTY_DIR CPL_VSIL_CURL_ALLOWED_EXTENSIONS=.tif,.TIF \
    GDAL_INGESTED_BYTES_AT_OPEN=4000000 GDAL_HTTP_MAX_RETRY=10 GDAL_HTTP_RETRY_DELAY=1 \
    VSI_CACHE=YES VSI_CACHE_SIZE=500000000 \
        gdalwarp -overwrite -q -ovr AUTO \
        -te "-$half" "-$half" "$half" "$half" -tr "$REGION_MPP" "$REGION_MPP" \
        -r bilinear "/vsicurl/$REGION_DEM_URL" "$out"
    echo "✓ Regional DEM done: $out ($(local_size "$out") bytes)"
}

echo "raw data dir: $RAW_DIR"
if [[ "$MODE" == region ]]; then
    crop_region_dem
else
    fetch "$DEM_URL" "$DEM_OUT" "DEM (PGDA LOLA 5 m/px Site04)"
fi

if [[ "$WANT_IMAGERY" -eq 1 ]]; then
    crop_imagery "$IMAGERY_URL"
else
    cat <<EOF

Imagery NOT fetched. Add it with:
  scripts/fetch_data.sh --with-imagery        # NAC south-pole crop (small, ~seconds)
The crop registers to the DEM grid; swap in finer imagery later via --imagery-url <URL>.
EOF
fi

echo
echo "Next: cargo run -p geo_pipeline -- bake --site ${SITE} --raw ${RAW_DIR#"$REPO_ROOT/"} --out assets"
