#!/bin/bash
# Regenerates the JPEG fixtures of tests/contract.rs and prints the table of expected checksums (paste it into `EXPECTED` there).
# Needs libjpeg(-turbo) tools (cjpeg, djpeg), ImageMagick `convert` and python3. The source is a picture made by the formula below (no real data in it);
# the "moli_*" files are real frames of the engine (Moli 1.1.25, Page.startScreencast, the page of the host's real-engine probe) and are copied, not made.
# Usage: ./make_jpeg_fixtures.sh   (run it inside this folder; it overwrites the generated files)
set -euo pipefail
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# a 61x37 picture with flat parts, a gradient, a diagonal edge and a little noise from a fixed formula (no random source)
convert -size 61x37 xc:white \
  -fill '#e53935' -draw 'rectangle 2,2 20,14' \
  -fill '#1e88e5' -draw 'rectangle 24,4 40,30' \
  -fill '#222222' -draw 'line 0,36 60,0' \
  -fill '#43a047' -draw 'circle 48,22 48,30' "$work/src.ppm"
convert "$work/src.ppm" \( -size 61x37 gradient:'#ff0000-#0000ff' -rotate 90 -resize 61x37! \) -compose blend -define compose:args=35 -composite "$work/src.ppm"
convert -size 1x1 xc:'#336699' "$work/one.ppm"
convert -size 8x8 gradient:'#000000-#ffffff' "$work/eight.ppm"
convert -size 96x48 gradient:'#ff8800-#0044ff' "$work/wide.ppm"
cjpeg -quality 85 -sample 1x1 "$work/src.ppm" > odd_61x37_444.jpg
cjpeg -quality 85 -sample 2x2 "$work/src.ppm" > odd_61x37_420.jpg
cjpeg -quality 85 -sample 2x1 "$work/src.ppm" > odd_61x37_422.jpg
cjpeg -quality 85 -sample 1x2 "$work/src.ppm" > odd_61x37_440.jpg
cjpeg -quality 70 -grayscale "$work/src.ppm" > grey_61x37.jpg
cjpeg -quality 85 -sample 2x2 -optimize "$work/src.ppm" > optimized_61x37_420.jpg
cjpeg -quality 85 -sample 2x2 -restart 1 "$work/wide.ppm" > restart_rows_96x48_420.jpg
cjpeg -quality 85 -sample 1x1 -restart 5B "$work/wide.ppm" > restart_blocks_96x48_444.jpg
cjpeg -quality 100 -sample 1x1 "$work/one.ppm" > one_pixel.jpg
cjpeg -quality 50 -grayscale "$work/eight.ppm" > grey_8x8.jpg
cjpeg -quality 85 -sample 2x2 -progressive "$work/src.ppm" > progressive_61x37.jpg
cjpeg -quality 85 -sample 2x2 -arithmetic "$work/src.ppm" > arithmetic_61x37.jpg
python3 - <<'PY'
import subprocess, hashlib
names = ["odd_61x37_444", "odd_61x37_420", "odd_61x37_422", "odd_61x37_440", "grey_61x37", "optimized_61x37_420", "restart_rows_96x48_420", "restart_blocks_96x48_444",
         "one_pixel", "grey_8x8", "moli_320x200_q60_420", "moli_320x200_q90_444"]
def fnv(path):
    # the reference decode: libjpeg's accurate integer IDCT, plain (box) chroma upsampling; the pixels as 0xAARRGGBB, FNV-1a over the u32 values, row after row
    out = subprocess.run(["djpeg", "-dct", "int", "-nosmooth", "-pnm", path], check=True, capture_output=True).stdout
    magic, rest = out.split(b"\n", 1)
    dims, rest = rest.split(b"\n", 1)
    _, rest = rest.split(b"\n", 1)  # max value
    w, h = map(int, dims.split())
    h64 = 0xcbf29ce484222325
    if magic == b"P5":
        px = [0xFF000000 | v << 16 | v << 8 | v for v in rest[: w * h]]
    else:
        px = [0xFF000000 | rest[3 * i] << 16 | rest[3 * i + 1] << 8 | rest[3 * i + 2] for i in range(w * h)]
    for p in px:
        h64 = ((h64 ^ p) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return w, h, h64
print("const EXPECTED: &[(&str, u32, u32, u64)] = &[")
for n in names:
    w, h, f = fnv(n + ".jpg")
    print(f'    ("{n}", {w}, {h}, 0x{f:016x}),')
print("];")
PY
