#!/usr/bin/env python3
"""Shaded preview PNG of an STL with a z-buffer (no depth-sorting artefacts).

    python3 gen/render.py in.stl out.png [elev azim]
"""
import math
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from stlcheck import load

W, H = 1200, 800
BG = np.array([22, 24, 31], float)
BASE = np.array([255, 107, 43], float)


def render(tris, elev=30.0, azim=-55.0):
    e, a = math.radians(elev), math.radians(azim)
    # camera basis: looking from direction d towards the centre
    d = np.array([math.cos(e) * math.cos(a), math.cos(e) * math.sin(a), math.sin(e)])
    right = np.cross([0, 0, 1], d); right /= np.linalg.norm(right)
    up = np.cross(d, right)
    pts = tris.reshape(-1, 3)
    c = (pts.min(0) + pts.max(0)) / 2
    p = pts - c
    sx, sy, depth = p @ right, p @ up, p @ d
    span = max(sx.max() - sx.min(), (sy.max() - sy.min()) * W / H)
    s = 0.86 * W / span
    X = (W / 2 + sx * s).reshape(-1, 3)
    Y = (H / 2 - sy * s).reshape(-1, 3)
    Z = depth.reshape(-1, 3)
    n = np.cross(tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0])
    n /= np.linalg.norm(n, axis=1, keepdims=True) + 1e-12
    light = d * 0.6 + up * 0.6 + right * 0.3; light /= np.linalg.norm(light)
    shade = 0.28 + 0.72 * np.abs(n @ light)
    img = np.tile(BG, (H, W, 1))
    zbuf = np.full((H, W), -np.inf)
    for i in range(len(tris)):
        x, y, z = X[i], Y[i], Z[i]
        x0, x1 = max(int(x.min()), 0), min(int(x.max()) + 1, W - 1)
        y0, y1 = max(int(y.min()), 0), min(int(y.max()) + 1, H - 1)
        if x0 > x1 or y0 > y1:
            continue
        gx, gy = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
        den = (y[1] - y[2]) * (x[0] - x[2]) + (x[2] - x[1]) * (y[0] - y[2])
        if abs(den) < 1e-9:
            continue
        w0 = ((y[1] - y[2]) * (gx - x[2]) + (x[2] - x[1]) * (gy - y[2])) / den
        w1 = ((y[2] - y[0]) * (gx - x[2]) + (x[0] - x[2]) * (gy - y[2])) / den
        w2 = 1 - w0 - w1
        inside = (w0 >= -1e-6) & (w1 >= -1e-6) & (w2 >= -1e-6)
        if not inside.any():
            continue
        zz = w0 * z[0] + w1 * z[1] + w2 * z[2]
        sub = zbuf[y0:y1 + 1, x0:x1 + 1]
        m = inside & (zz > sub)
        sub[m] = zz[m]
        img[y0:y1 + 1, x0:x1 + 1][m] = BASE * shade[i]
    return Image.fromarray(np.clip(img, 0, 255).astype(np.uint8))


if __name__ == "__main__":
    t = load(sys.argv[1])
    elev, azim = (float(sys.argv[3]), float(sys.argv[4])) if len(sys.argv) > 4 else (30, -55)
    render(t, elev, azim).save(sys.argv[2])
