#!/usr/bin/env bash
set -euo pipefail

# Generates ../motion/ink-sequence.webp: a 25-frame ink-spread sprite for the
# landing's scroll-driven ink transition (the CodyHouse sprite+steps technique,
# rebuilt from this directory's cleared brush scans — their demo asset carries
# no license or footage credit, so it is not vendored).
#
# Composition per frame, learned from studying real ink footage: a centre blot
# growing, plus ink creeping inward from corners and edges, everything keeping
# the scans' semi-transparent feather (no thresholding), displaced by one
# fixed noise field so the edges finger instead of scaling cleanly.
#
# Deterministic. Requires ImageMagick 7. Rerun by hand when the source scans
# change; the sprite is checked in.

cd "$(dirname "$0")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

frame_px=512
frames=25

magick ink-blot.webp -alpha extract "$work/blot.png"
magick ink-square.webp -alpha extract "$work/square.png"
magick -seed 916 -size ${frame_px}x${frame_px} plasma:gray20-gray80 \
  -colorspace gray -blur 0x3 -auto-level "$work/disp-x.png"
magick -seed 407 -size ${frame_px}x${frame_px} plasma:gray20-gray80 \
  -colorspace gray -blur 0x3 -auto-level "$work/disp-y.png"
magick "$work/disp-x.png" "$work/disp-y.png" "$work/disp-x.png" \
  -combine "$work/disp.png"

# stamp <src> <scale%> <rotation> <cx%> <cy%> <gain> <out>
stamp() {
  local src="$1" scale="$2" rot="$3" cx="$4" cy="$5" gain="$6" out="$7"
  local size cxpx cypx
  size=$(python3 -c "print(max(1, int($frame_px * $scale / 100)))")
  cxpx=$(python3 -c "print(int($frame_px * $cx / 100 - $frame_px / 2))")
  cypx=$(python3 -c "print(int($frame_px * $cy / 100 - $frame_px / 2))")
  magick "$work/$src" -resize "${size}x${size}!" -background black \
    -rotate "$rot" -gravity center -extent ${frame_px}x${frame_px} \
    -roll "$(printf '%+d%+d' "$cxpx" "$cypx")" \
    -evaluate multiply "$gain" "$out"
}

# grow <g> <from> <to>: eased interpolation of a stamp size across the run.
grow() {
  python3 -c "g=$1;print(round($2 + ($3 - $2) * (g ** 2.3), 1))"
}

for k in $(seq 0 $((frames - 1))); do
  g=$(python3 -c "print($k / ($frames - 1))")
  layers=()

  # Centre blot, present from the first inked frame.
  stamp blot.png "$(grow "$g" 5 170)" 20 48 44 1.0 "$work/s1.png"
  layers+=("$work/s1.png")

  # Bottom-left corner wash.
  if python3 -c "exit(0 if $g > 0.18 else 1)"; then
    stamp square.png "$(grow "$g" 16 250)" 155 -6 106 0.92 "$work/s2.png"
    layers+=("$work/s2.png")
  fi
  # Top-right edge wash.
  if python3 -c "exit(0 if $g > 0.34 else 1)"; then
    stamp blot.png "$(grow "$g" 14 225)" 250 88 -8 0.88 "$work/s3.png"
    layers+=("$work/s3.png")
  fi
  # Top-left corner.
  if python3 -c "exit(0 if $g > 0.50 else 1)"; then
    stamp square.png "$(grow "$g" 12 215)" 70 -4 -6 0.85 "$work/s4.png"
    layers+=("$work/s4.png")
  fi
  # Right edge, closing the last gap.
  if python3 -c "exit(0 if $g > 0.62 else 1)"; then
    stamp blot.png "$(grow "$g" 12 205)" 330 108 64 0.85 "$work/s5.png"
    layers+=("$work/s5.png")
  fi

  # Union of feathered stamps, densified as the run progresses so covered
  # paper goes fully opaque, fingered by the fixed displacement field.
  density=$(python3 -c "print(round(1.0 + 1.2 * $g ** 3.2, 3))")
  magick "${layers[@]}" -evaluate-sequence Max \
    -evaluate multiply "$density" \
    "$work/disp.png" -compose displace -define compose:args=12x12 -composite \
    -blur 0x0.5 "$work/shape.png"
  magick -size ${frame_px}x${frame_px} xc:white "$work/shape.png" \
    -alpha off -compose CopyOpacity -composite \
    "$(printf "%s/frame-%02d.png" "$work" "$k")"
done

# The last frame is full coverage by contract.
magick -size ${frame_px}x${frame_px} xc:white \
  "$(printf "%s/frame-%02d.png" "$work" $((frames - 1)))"

magick "$work"/frame-*.png +append PNG64:"$work/sprite.png"
magick "$work/sprite.png" -quality 78 -define webp:alpha-quality=88 \
  ../motion/ink-sequence.webp
identify ../motion/ink-sequence.webp
