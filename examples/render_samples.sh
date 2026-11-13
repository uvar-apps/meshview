#!/usr/bin/env bash
# Correct meshview invocations for the shipped sample assets.
#
# Two framing rules to keep in mind:
#
# 1. One sheet = one camera. The three samples live at wildly different unit
#    scales (bunny ≈ 0.16 units tall, dragon ≈ 200 units wide, crambin SES
#    ≈ 30 Å), and focus/zoom are shared across a sheet by design — so each
#    mesh gets its own invocation with its own zoom.
#
# 2. `--aa` halves on-screen magnification at fixed `--zoom`: tiles render at
#    zoom × size × aa supersampled pixels and then filter down to `size`, so
#    the final sheet shows zoom/aa pixels per world unit (this mirrors the
#    Python original). The zooms below are chosen for `--aa 2`.
#
# Usage:
#   cargo build --release
#   examples/render_samples.sh [output-dir]     # default: ./samples_out
set -euo pipefail

MV="${CARGO_BIN_EXE_meshview:-./target/release/meshview}"
OUT="${1:-./samples_out}"
S="assets/samples"
mkdir -p "$OUT"

# Effective magnification at aa 2 is zoom/2 px per world unit; targets below
# put the widest span at ~800–850 px inside a 900 px tile.

echo "# Stanford bunny — 0.156 units tall; zoom 10000 -> ~780 px at aa 2"
"$MV" "$S/stanford-bunny.obj"   -o "$OUT/bunny.png"     --zoom 10000 --size 900 --aa 2 --mask "$OUT/bunny_mask.png"

echo "# XYZRGB dragon — 202 units wide; zoom 8 -> ~810 px at aa 2"
"$MV" "$S/xyzrgb_dragon.obj"    -o "$OUT/dragon.png"    --zoom 8     --size 900 --aa 2 --mask "$OUT/dragon_mask.png"

echo "# crambin SES (molses) — 30 Å across; zoom 50 -> ~760 px at aa 2"
"$MV" "$S/1crn_ses_0.5A.obj"    -o "$OUT/1crn_full.png" --zoom 50    --size 900 --aa 2 --label "1CRN SES, grid 0.5 A"

echo "# crambin SES crevice close-up — the region meshview was built to inspect"
"$MV" "$S/1crn_ses_0.5A.obj"    -o "$OUT/1crn_crevice.png" \
    --zoom 110 --focus 4.29,5.05,4.55 --size 900 --aa 2 --label "crevice @ (4.29, 5.05, 4.55)"

echo "# Self-compare sanity check: the same file twice must produce identical panels"
"$MV" "$S/1crn_ses_0.5A.obj" "$S/1crn_ses_0.5A.obj" \
    -o "$OUT/1crn_selfcompare.png" --zoom 110 --focus 4.29,5.05,4.55 --size 620 \
    --label "run A" --label "run B"

echo
echo "wrote: $(ls "$OUT")"
