#!/usr/bin/env bash
#
# Transcode benchmark script — mirrors nostube-transcode settings for VideoToolbox (Apple Silicon)
#
# Usage: ./bench_transcode.sh <input_video>
#
# Runs all resolution × codec combinations using the same ffmpeg flags as the app,
# then prints a summary table of resulting bitrates via ffprobe.

set -euo pipefail

INPUT="${1:?Usage: $0 <input_video>}"
FFMPEG="${FFMPEG:-ffmpeg}"
FFPROBE="${FFPROBE:-ffprobe}"
OUTDIR="bench_output_$(date +%Y%m%d_%H%M%S)"

mkdir -p "$OUTDIR"

# ---------------------------------------------------------------------------
# Resolution / quality matrix  (from transform.rs + handler.rs)
#
# Uses -b:v (average bitrate) instead of -q:v because VideoToolbox's
# quality-based VBR produces unpredictably high bitrates.
# -b:v reliably constrains output to the target.
# ---------------------------------------------------------------------------

# Format: "label height target_bitrate audio_bitrate"

# H.264 bitrate targets (higher bitrates needed for equivalent quality)
VARIANTS_H264=(
  "240p  240  450k   64k"
  "360p  360  900k   96k"
  "480p  480  1350k  128k"
  "720p  720  2800k  128k"
  "1080p 1080 5000k  128k"
)

# H.265 bitrate targets (more efficient codec)
VARIANTS_H265=(
  "240p  240  300k   64k"
  "360p  360  600k   96k"
  "480p  480  900k   128k"
  "720p  720  1875k  128k"
  "1080p 1080 3375k  128k"
)

CODECS=(
  "h264  h264_videotoolbox"
  "h265  hevc_videotoolbox"
)

# Get input height to skip resolutions higher than source
INPUT_HEIGHT=$("$FFPROBE" -v error -select_streams v:0 \
  -show_entries stream=height -of csv=p=0 "$INPUT" | head -1)

echo "Input: $INPUT"
echo "Input height: ${INPUT_HEIGHT}p"
echo "Output dir: $OUTDIR"
echo ""

# ---------------------------------------------------------------------------
# Transcode loop
# ---------------------------------------------------------------------------
declare -a OUTPUT_FILES=()

for codec_line in "${CODECS[@]}"; do
  read -r codec_label encoder <<< "$codec_line"

  # Select variant array for this codec
  if [[ "$codec_label" == "h264" ]]; then
    variants=("${VARIANTS_H264[@]}")
  else
    variants=("${VARIANTS_H265[@]}")
  fi

  for variant in "${variants[@]}"; do
    read -r res_label height target_br audio_br <<< "$variant"

    # Skip if input is smaller than target
    if (( height > INPUT_HEIGHT )); then
      echo "SKIP  ${res_label}_${codec_label} — input ${INPUT_HEIGHT}p < ${height}p"
      continue
    fi

    outfile="${OUTDIR}/${res_label}_${codec_label}.mp4"

    # Build scale filter — use scale_vt for VideoToolbox HW scaling
    vf="scale_vt=w=-2:h=${height}"

    # hvc1 tag for H.265 Safari/iOS compat
    tag_args=()
    if [[ "$codec_label" == "h265" ]]; then
      tag_args=(-tag:v hvc1)
    fi

    echo "RUN   ${res_label}_${codec_label}  (b:v ${target_br}, audio ${audio_br})"

    "$FFMPEG" -y -nostdin \
      -hwaccel videotoolbox \
      -hwaccel_output_format videotoolbox_vld \
      -threads 0 \
      -i "$INPUT" \
      -vf "$vf" \
      -c:v "$encoder" \
      ${tag_args[@]+"${tag_args[@]}"} \
      -b:v "$target_br" \
      -c:a aac -b:a "$audio_br" \
      -movflags +faststart \
      "$outfile" \
      2>&1 | tail -1

    OUTPUT_FILES+=("$outfile")
    echo ""
  done
done

# ---------------------------------------------------------------------------
# Results table via ffprobe
# ---------------------------------------------------------------------------
echo ""
echo "======================================================================"
echo "  RESULTS"
echo "======================================================================"
printf "%-22s  %8s  %8s  %10s  %8s  %8s\n" \
  "FILE" "V-KBPS" "A-KBPS" "SIZE-MB" "DURATION" "TARGET"

for f in "${OUTPUT_FILES[@]}"; do
  if [[ ! -f "$f" ]]; then
    continue
  fi

  basename=$(basename "$f" .mp4)

  # Extract video bitrate, audio bitrate, duration
  v_bps=$("$FFPROBE" -v error -select_streams v:0 \
    -show_entries stream=bit_rate -of csv=p=0 "$f" 2>/dev/null || echo "N/A")
  a_bps=$("$FFPROBE" -v error -select_streams a:0 \
    -show_entries stream=bit_rate -of csv=p=0 "$f" 2>/dev/null || echo "N/A")
  duration=$("$FFPROBE" -v error \
    -show_entries format=duration -of csv=p=0 "$f" 2>/dev/null || echo "N/A")
  file_size=$(stat -f%z "$f" 2>/dev/null || stat --printf=%s "$f" 2>/dev/null || echo "0")

  # Convert bps to kbps
  if [[ "$v_bps" =~ ^[0-9]+$ ]]; then
    v_kbps=$(( v_bps / 1000 ))
  else
    # Fallback: compute from format bitrate
    fmt_bps=$("$FFPROBE" -v error \
      -show_entries format=bit_rate -of csv=p=0 "$f" 2>/dev/null || echo "0")
    if [[ "$fmt_bps" =~ ^[0-9]+$ ]]; then
      v_kbps=$(( fmt_bps / 1000 ))
    else
      v_kbps="N/A"
    fi
  fi

  if [[ "$a_bps" =~ ^[0-9]+$ ]]; then
    a_kbps=$(( a_bps / 1000 ))
  else
    a_kbps="N/A"
  fi

  # Size in MB
  if [[ "$file_size" =~ ^[0-9]+$ ]] && (( file_size > 0 )); then
    size_mb=$(awk "BEGIN { printf \"%.1f\", $file_size / 1048576 }")
  else
    size_mb="N/A"
  fi

  # Duration formatted
  if [[ "$duration" =~ ^[0-9.]+$ ]]; then
    dur_fmt=$(awk "BEGIN { s=int($duration); printf \"%d:%02d\", s/60, s%60 }")
  else
    dur_fmt="N/A"
  fi

  # Recover target bitrate from filename
  res_label="${basename%%_*}"
  codec_label="${basename##*_}"
  target_used="N/A"
  if [[ "$codec_label" == "h264" ]]; then
    lookup_variants=("${VARIANTS_H264[@]}")
  else
    lookup_variants=("${VARIANTS_H265[@]}")
  fi
  for variant in "${lookup_variants[@]}"; do
    read -r vl vh vt va <<< "$variant"
    if [[ "$vl" == "$res_label" ]]; then
      target_used="$vt"
      break
    fi
  done

  printf "%-22s  %8s  %8s  %10s  %8s  %8s\n" \
    "$basename" "$v_kbps" "$a_kbps" "$size_mb" "$dur_fmt" "$target_used"
done

echo "======================================================================"
echo ""
echo "Output directory: $OUTDIR"
