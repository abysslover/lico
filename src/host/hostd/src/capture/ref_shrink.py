"""Independent reference for the box filter of the agent copy (used by the tests of src/host/hostd/src/capture; run as python3 -I FILE ...).

usage: ref_shrink.py IN.rgb SW SH DW DH [x,y ...]
IN.rgb is raw 8-bit RGB, SW x SH. For each destination point x,y (all of them when none is given) it prints "x y r g b": the mean of the
source pixels the destination pixel covers, weighted by the exact area of overlap (Fractions, no floating point), rounded to nearest, halves up.
"""
import math
import sys
from fractions import Fraction

data = open(sys.argv[1], "rb").read()
sw, sh, dw, dh = (int(v) for v in sys.argv[2:6])
assert len(data) == sw * sh * 3
points = [tuple(int(n) for n in p.split(",")) for p in sys.argv[6:]] or [(x, y) for y in range(dh) for x in range(dw)]


def spans(i, src, dst):
    """The source pixels destination pixel i covers on one axis, with the exact length of the overlap (a Fraction of a source pixel)."""
    lo, hi = Fraction(i * src, dst), Fraction((i + 1) * src, dst)
    out = []
    for s in range(math.floor(lo), math.ceil(hi)):
        out.append((s, min(hi, s + 1) - max(lo, s)))
    return out


for (x, y) in points:
    sx, sy = spans(x, sw, dw), spans(y, sh, dh)
    area = sum(w for _, w in sx) * sum(w for _, w in sy)
    rgb = []
    for c in range(3):
        total = Fraction(0)
        for yy, wy in sy:
            for xx, wx in sx:
                total += wx * wy * data[(yy * sw + xx) * 3 + c]
        rgb.append(math.floor(total / area + Fraction(1, 2)))
    print(x, y, *rgb)
