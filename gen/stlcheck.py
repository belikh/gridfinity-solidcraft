#!/usr/bin/env python3
"""Measure an STL: bounding box, volume, and the X extent of the solid at given heights.

    python3 gen/stlcheck.py out/bin.stl 0 0.4 0.8 2.6 4.75 21 23
"""
import struct
import sys

import numpy as np


def load(path):
    data = open(path, "rb").read()
    if data[:5] == b"solid" and b"facet" in data[:300]:
        v = [list(map(float, l.split()[1:4])) for l in data.decode().splitlines() if l.strip().startswith("vertex")]
        return np.array(v).reshape(-1, 3, 3)
    n = struct.unpack("<I", data[80:84])[0]
    a = np.frombuffer(data[84:84 + n * 50], dtype=np.dtype([("n", "<f4", 3), ("v", "<f4", (3, 3)), ("a", "<u2")]))
    return a["v"].astype(float)


def volume(t):
    return float(np.einsum("ij,ij->i", t[:, 0], np.cross(t[:, 1], t[:, 2])).sum() / 6)


def section_x(t, z, y=None):
    """Sorted x of edge crossings at height z (on the line y if given): pairs are solid spans."""
    xs = []
    for tri in t:
        zs = tri[:, 2]
        if zs.min() > z or zs.max() < z or zs.min() == zs.max():
            continue
        pts = []
        for i in range(3):
            p, q = tri[i], tri[(i + 1) % 3]
            if (p[2] - z) * (q[2] - z) < 0:
                f = (z - p[2]) / (q[2] - p[2])
                pts.append(p + f * (q - p))
        if len(pts) == 2 and y is not None:
            a, b = pts
            if (a[1] - y) * (b[1] - y) <= 0 and a[1] != b[1]:
                f = (y - a[1]) / (b[1] - a[1])
                xs.append(round(float(a[0] + f * (b[0] - a[0])), 3))
    return sorted(set(xs))


if __name__ == "__main__":
    t = load(sys.argv[1])
    lo, hi = t.reshape(-1, 3).min(0), t.reshape(-1, 3).max(0)
    print(f"triangles {len(t)}  bbox {lo.round(3).tolist()} .. {hi.round(3).tolist()}  volume {volume(t):.1f} mm3")
    yline = float(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2].startswith("y=") is False else None
    args = sys.argv[2:]
    y = (lo[1] + hi[1]) / 2
    for a in args:
        if a.startswith("y="):
            y = float(a[2:]); continue
        z = float(a)
        print(f"z={z:6.2f} y={y:.2f}: x crossings {section_x(t, z, y)}")
