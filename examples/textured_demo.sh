#!/usr/bin/env bash
# Live demo of --textured on a real textured model fetched from the web.
#
# The model is the assimp test-suite spider (test/models/OBJ/spider.obj):
# 762 v / 302 vt / 1368 faces, mtllib spider.mtl, four referenced materials
# each with a map_Kd JPG — including Windows-backslash map paths, which
# objserde/meshview normalize. Fetched here on demand and NOT committed to
# either repo: the individual textures' original provenance is unclear (the
# assimp project ships them as test data), so we keep the same discipline as
# the Stanford assets — GitHub may carry them, our crates do not.
#
# Usage:
#   cargo build --release
#   examples/textured_demo.sh [work-dir]     # default: /tmp/meshview-textured-demo
set -euo pipefail

MV="${CARGO_BIN_EXE_meshview:-./target/release/meshview}"
WORK="${1:-/tmp/meshview-textured-demo}"
BASE="https://raw.githubusercontent.com/assimp/assimp/master/test/models/OBJ"

mkdir -p "$WORK"
cd "$WORK"
for f in spider.obj spider.mtl wal67ar_small.jpg wal69ar_small.jpg SpiderTex.jpg drkwood2.jpg engineflare1.jpg; do
    [ -s "$f" ] || curl -fsSL -O "$BASE/$f"
done

echo "# textured (map_Kd x Kd x Lambert)"
"$MV" spider.obj -o spider_textured.png --zoom 8 --size 900 --aa 2 --textured \
    --label "assimp spider, map_Kd x Kd x Lambert"

echo "# same camera, no texturing (for comparison)"
"$MV" spider.obj -o spider_plain.png --zoom 8 --size 900 --aa 2 \
    --label "assimp spider, geometry only"

echo
echo "wrote: $(ls spider_textured.png spider_plain.png)"
