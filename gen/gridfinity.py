#!/usr/bin/env python3
"""
Gridfinity bins and baseplates as SolidCraft command scripts.

    python3 gen/gridfinity.py bin 2 1 3 --magnets > bin-2x1x3.json
    python3 gen/gridfinity.py baseplate 3 2 > baseplate-3x2.json
    solidcraft-cli run bin-2x1x3.json --out bin.stl

This is the reference for the plugin: the Rust plugin issues the same commands
in the same order. Dimensions: research/SPEC.md (Gridfinity, as in
gridfinity-rebuilt-openscad). All lengths in mm; the bin sits with its first
cell's corner at the origin, z up.
"""

import argparse
import json
import sys

PITCH = 42.0
UNIT_H = 7.0
GAP = 0.5                          # between neighbouring bins
FOOT = (0.8, 1.8, 2.15)            # 45° out, straight, 45° out (bottom up)
FOOT_TOP_R = 3.75
BASE_H = 7.0                       # feet + bridge
FLOOR_Z = 6.0                      # top of the floor inside the bin (assumption, see NOTES.md)
WALL = 0.95
LIP = (0.7, 1.8, 1.9)              # from the inner tip: 45° out, straight, 45° out
LIP_FLAT = 0.3                     # flat on the very top instead of a knife edge
LIP_SUPPORT = 1.2
DIVIDER = 1.2
HOLE_OFFSET = 13.0                 # hole centres from the cell centre (±)
MAGNET_D, MAGNET_DEPTH = 6.5, 2.4
SCREW_D, SCREW_DEPTH = 3.0, 6.0
PLATE_PROFILE = (0.7, 1.8, 2.15)   # pocket, bottom up
PLATE_R = 4.0


def C(cmd, **p):
    return {"command": cmd, "params": p}


def rrect(cx, cy, w, d, r):
    return C("sketch.rectangle.rounded", p0=[round(cx - w / 2, 4), round(cy - d / 2, 4)],
             p1=[round(cx + w / 2, 4), round(cy + d / 2, 4)], radius=round(r, 4))


def sketch(z, name, items):
    return [C("sketch.create", plane="XY", offset=round(z, 4), name=name), *items, C("sketch.finish")]


def extrude(name, dist, op, body, taper=None, direction=None):
    p = {"sketch": name, "profiles": "all", "distance": round(dist, 4), "operation": op}
    if op == "new":
        p["body_name"] = body
    else:
        p["targets"] = [body]
    if taper is not None:
        p["taper"] = taper
    if direction:
        p["direction"] = direction
    return C("solid.extrude", **p)


def cells(nx, ny):
    for i in range(nx):
        for j in range(ny):
            yield PITCH * i + PITCH / 2, PITCH * j + PITCH / 2


def bin_commands(nx, ny, u, magnets=False, screws=False, div_x=0, div_y=0, lip=True, body="Bin"):
    cmds = []
    top = FOOT_TOP_R
    a, b, c = FOOT
    s_bottom = (PITCH - GAP) - 2 * (a + c)            # 35.6
    r_bottom = top - (a + c)                           # 0.8
    s_mid, r_mid = s_bottom + 2 * a, r_bottom + a      # 37.2, 1.6
    foot_h = a + b + c                                 # 4.75
    # Body first: one block over all cells, from the top of the feet to the top of the walls.
    W, D = PITCH * nx - GAP, PITCH * ny - GAP
    cx, cy = PITCH * nx / 2, PITCH * ny / 2
    height = UNIT_H * u
    block_top = height + (sum(LIP) - LIP_FLAT if lip else 0)
    cmds += sketch(foot_h, "Body", [rrect(cx, cy, W, D, top)])
    cmds.append(extrude("Body", block_top - foot_h, "new", body))
    # Feet, built downwards from the block so every piece joins the same body:
    # 2.15 at 45° in, 1.8 straight, 0.8 at 45° in.
    s_top = PITCH - GAP                                # 41.5
    cmds += sketch(foot_h, "Feet3", [rrect(x, y, s_top, s_top, top) for x, y in cells(nx, ny)])
    cmds.append(extrude("Feet3", c, "join", body, taper=-45, direction="negative"))
    cmds += sketch(a + b, "Feet2", [rrect(x, y, s_mid, s_mid, r_mid) for x, y in cells(nx, ny)])
    cmds.append(extrude("Feet2", b, "join", body, direction="negative"))
    cmds += sketch(a, "Feet1", [rrect(x, y, s_mid, s_mid, r_mid) for x, y in cells(nx, ny)])
    cmds.append(extrude("Feet1", a, "join", body, taper=-45, direction="negative"))
    # Cavity, cut from the top down.
    tip = sum((LIP[0], LIP[2]))                        # lip's inner tip: 2.6 in from the outside
    if lip:
        z = block_top
        inset = LIP_FLAT
        # 45° in, down to the end of the upper slope
        d1 = LIP[2] - LIP_FLAT
        cmds += sketch(z, "Lip1", [rrect(cx, cy, W - 2 * inset, D - 2 * inset, max(0.2, top - inset))])
        cmds.append(extrude("Lip1", d1, "cut", body, taper=-45, direction="negative"))
        z -= d1
        inset = LIP[2]
        cmds += sketch(z, "Lip2", [rrect(cx, cy, W - 2 * inset, D - 2 * inset, top - inset)])
        cmds.append(extrude("Lip2", LIP[1], "cut", body, direction="negative"))
        z -= LIP[1]
        cmds += sketch(z, "Lip3", [rrect(cx, cy, W - 2 * inset, D - 2 * inset, top - inset)])
        cmds.append(extrude("Lip3", LIP[0], "cut", body, taper=-45, direction="negative"))
        z -= LIP[0]                                    # = height, inner tip at `tip`
        # support below the tip, then 45° back out to the wall
        cmds += sketch(z, "Support1", [rrect(cx, cy, W - 2 * tip, D - 2 * tip, top - tip)])
        cmds.append(extrude("Support1", LIP_SUPPORT, "cut", body, direction="negative"))
        z -= LIP_SUPPORT
        slope = tip - WALL
        cmds += sketch(z, "Support2", [rrect(cx, cy, W - 2 * tip, D - 2 * tip, top - tip)])
        cmds.append(extrude("Support2", slope, "cut", body, taper=45, direction="negative"))
        z -= slope
    else:
        z = height
    cmds += sketch(z, "Cavity", [rrect(cx, cy, W - 2 * WALL, D - 2 * WALL, top - WALL)])
    cmds.append(extrude("Cavity", z - FLOOR_Z, "cut", body, direction="negative"))
    # Dividers: walls across the cavity, from the floor to just under the lip support.
    div_top = z
    inner_w, inner_d = W - 2 * WALL, D - 2 * WALL
    x0, y0 = cx - inner_w / 2, cy - inner_d / 2
    for k in range(1, div_x + 1):
        x = x0 + inner_w * k / (div_x + 1) - DIVIDER / 2
        cmds.append(C("solid.box", corner=[round(x, 4), round(y0, 4), FLOOR_Z], length=DIVIDER,
                      width=round(inner_d, 4), height=round(div_top - FLOOR_Z, 4), operation="join", targets=[body]))
    for k in range(1, div_y + 1):
        y = y0 + inner_d * k / (div_y + 1) - DIVIDER / 2
        cmds.append(C("solid.box", corner=[round(x0, 4), round(y, 4), FLOOR_Z], length=round(inner_w, 4),
                      width=DIVIDER, height=round(div_top - FLOOR_Z, 4), operation="join", targets=[body]))
    # Holes in the bottom of every foot.
    holes = []
    if magnets:
        holes.append(("Magnets", MAGNET_D, MAGNET_DEPTH))
    if screws:
        holes.append(("Screws", SCREW_D, SCREW_DEPTH))
    for name, dia, depth in holes:
        pts = [C("sketch.circle.center", center=[round(x + sx * HOLE_OFFSET, 4), round(y + sy * HOLE_OFFSET, 4)], diameter=dia)
               for x, y in cells(nx, ny) for sx in (-1, 1) for sy in (-1, 1)]
        cmds += sketch(0, name, pts)
        cmds.append(extrude(name, depth, "cut", body, direction="positive"))
    return cmds


def baseplate_commands(nx, ny, magnets=False, body="Baseplate"):
    a, b, c = PLATE_PROFILE
    depth = a + b + c                                  # 4.65
    extra = 0.35 if not magnets else MAGNET_DEPTH + 0.6  # floor under the pockets
    H = depth + extra
    W, D = PITCH * nx, PITCH * ny
    cx, cy = W / 2, D / 2
    cmds = sketch(0, "Plate", [rrect(cx, cy, W, D, PLATE_R)])
    cmds.append(extrude("Plate", H, "new", body))
    # The spec's pockets meet at knife edges; the kernel can't taper a profile that touches its
    # neighbour, so the top of each pocket keeps a 0.05 mm rim (the 45° starts 0.05 lower).
    rim = 0.05
    s_top, r_top = PITCH - 2 * rim, PLATE_R - rim
    s_mid, r_mid = PITCH - 2 * c, PLATE_R - c          # 37.7, 1.85
    cmds += sketch(H, "Pocket1", [rrect(x, y, s_top, s_top, r_top) for x, y in cells(nx, ny)])
    cmds.append(extrude("Pocket1", c - rim, "cut", body, taper=-45, direction="negative"))
    cmds += sketch(H - c + rim, "Pocket2", [rrect(x, y, s_mid, s_mid, r_mid) for x, y in cells(nx, ny)])
    cmds.append(extrude("Pocket2", b, "cut", body, direction="negative"))
    cmds += sketch(H - c + rim - b, "Pocket3", [rrect(x, y, s_mid, s_mid, r_mid) for x, y in cells(nx, ny)])
    cmds.append(extrude("Pocket3", a, "cut", body, taper=-45, direction="negative"))
    if magnets:
        floor = H - (depth - rim)                      # bottom of the pockets
        pts = [C("sketch.circle.center", center=[round(x + sx * HOLE_OFFSET, 4), round(y + sy * HOLE_OFFSET, 4)], diameter=MAGNET_D)
               for x, y in cells(nx, ny) for sx in (-1, 1) for sy in (-1, 1)]
        cmds += sketch(floor, "Magnets", pts)
        cmds.append(extrude("Magnets", MAGNET_DEPTH, "cut", body, direction="negative"))
    return cmds


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="kind", required=True)
    b = sub.add_parser("bin")
    b.add_argument("nx", type=int); b.add_argument("ny", type=int); b.add_argument("units", type=int)
    b.add_argument("--magnets", action="store_true"); b.add_argument("--screws", action="store_true")
    b.add_argument("--div-x", type=int, default=0); b.add_argument("--div-y", type=int, default=0)
    b.add_argument("--no-lip", action="store_true")
    p = sub.add_parser("baseplate")
    p.add_argument("nx", type=int); p.add_argument("ny", type=int)
    p.add_argument("--magnets", action="store_true")
    a = ap.parse_args(argv)
    if a.kind == "bin":
        cmds = bin_commands(a.nx, a.ny, a.units, a.magnets, a.screws, a.div_x, a.div_y, not a.no_lip)
    else:
        cmds = baseplate_commands(a.nx, a.ny, a.magnets)
    json.dump({"commands": cmds}, sys.stdout, indent=1)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
