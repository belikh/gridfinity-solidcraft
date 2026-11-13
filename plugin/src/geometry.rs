//! Gridfinity bins and baseplates as a list of SolidCraft commands.
//!
//! The shapes are those of `gen/gridfinity.py`, whose output was checked against the spec in
//! research/SPEC.md by measuring the exported STL. The plugin builds them differently (one
//! cell made and repeated, so big grids stay within SolidCraft's 256 calls and 30 s a run);
//! `gen/volumes.json` holds the volumes SolidCraft measured for both, which must match.

use serde_json::{json, Value};

pub const PITCH: f64 = 42.0;
/// Half-grid bins: four feet of 21 mm in one 42 mm baseplate pocket.
pub const HALF_PITCH: f64 = 21.0;
pub const UNIT_H: f64 = 7.0;
const GAP: f64 = 0.5;
const FOOT: (f64, f64, f64) = (0.8, 1.8, 2.15);
const FOOT_TOP_R: f64 = 3.75;
const FLOOR_Z: f64 = 6.0;
const WALL: f64 = 0.95;
const LIP: (f64, f64, f64) = (0.7, 1.8, 1.9);
const LIP_FLAT: f64 = 0.3;
const LIP_SUPPORT: f64 = 1.2;
const DIVIDER: f64 = 1.2;
const HOLE_OFFSET: f64 = 13.0;
pub(crate) const MAGNET_D: f64 = 6.5;
pub(crate) const MAGNET_DEPTH: f64 = 2.4;
const SCREW_D: f64 = 3.0;
const SCREW_DEPTH: f64 = 6.0;
pub(crate) const PLATE_PROFILE: (f64, f64, f64) = (0.7, 1.8, 2.15);
pub(crate) const PLATE_R: f64 = 4.0;
pub(crate) const PLATE_RIM: f64 = 0.05;

#[derive(Debug, Clone, PartialEq)]
pub struct BinOptions {
    pub nx: u32,
    pub ny: u32,
    /// Cell size in mm along X and Y: PITCH (full grid), HALF_PITCH, or a custom size.
    pub pitch: [f64; 2],
    pub units: u32,
    pub magnets: bool,
    pub screws: bool,
    pub div_x: u32,
    pub div_y: u32,
    pub lip: bool,
    /// Filled up to the top of the walls (no cavity, no dividers), for cutting custom holders.
    pub solid: bool,
    pub origin: [f64; 2],
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BaseplateOptions {
    pub nx: u32,
    pub ny: u32,
    /// Pocket size in mm along X and Y: PITCH (full grid), HALF_PITCH, or a custom size.
    pub pitch: [f64; 2],
    /// Frame only: the pockets go right through (no floor, so no magnet holes).
    pub open: bool,
    pub magnets: bool,
    pub origin: [f64; 2],
    pub name: String,
}

pub(crate) fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

pub(crate) fn cmd(command: &str, params: Value) -> Value {
    json!({"command": command, "params": params})
}

pub(crate) fn rrect(cx: f64, cy: f64, w: f64, d: f64, r: f64) -> Value {
    cmd(
        "sketch.rectangle.rounded",
        json!({"p0": [r4(cx - w / 2.0), r4(cy - d / 2.0)], "p1": [r4(cx + w / 2.0), r4(cy + d / 2.0)], "radius": r4(r)}),
    )
}

pub(crate) fn circle(x: f64, y: f64, d: f64) -> Value {
    cmd("sketch.circle.center", json!({"center": [r4(x), r4(y)], "diameter": d}))
}

pub(crate) fn sketch(out: &mut Vec<Value>, z: f64, name: &str, items: Vec<Value>) {
    out.push(cmd("sketch.create", json!({"plane": "XY", "offset": r4(z), "name": name})));
    out.extend(items);
    out.push(cmd("sketch.finish", json!({})));
}

pub(crate) enum Op<'a> {
    New(&'a str),
    Join(&'a str),
    Cut(&'a str),
}

/// `dir`: "negative" (down), "positive", or "" for the default.
pub(crate) fn extrude(sketch: &str, distance: f64, op: Op, taper: Option<f64>, dir: &str) -> Value {
    let mut p = json!({"sketch": sketch, "profiles": "all", "distance": r4(distance)});
    match op {
        Op::New(b) => {
            p["operation"] = json!("new");
            p["body_name"] = json!(b);
        }
        Op::Join(b) => {
            p["operation"] = json!("join");
            p["targets"] = json!([b]);
        }
        Op::Cut(b) => {
            p["operation"] = json!("cut");
            p["targets"] = json!([b]);
        }
    }
    if let Some(t) = taper {
        p["taper"] = json!(t);
    }
    if !dir.is_empty() {
        p["direction"] = json!(dir);
    }
    cmd("solid.extrude", p)
}

/// Hole positions along one axis inside a foot centred at `c` (width `pitch`): the 42 mm grid's
/// magnet spots, ±13 mm from each 42 mm cell centre counted from `origin`. A full foot gets two
/// per axis (four holes); a half-grid foot gets the one that lines up with a baseplate magnet.
/// A custom size has no 42 mm grid to line up with: two holes 8 mm in from the cell's edges
/// when there is room, otherwise one in the middle (bins and baseplates of the same size agree).
fn hole_axis(c: f64, origin: f64, pitch: f64) -> Vec<f64> {
    if pitch != PITCH && pitch != HALF_PITCH {
        let off = pitch / 2.0 - (PITCH / 2.0 - HOLE_OFFSET);
        return if off >= MAGNET_D { vec![c - off, c + off] } else { vec![c] };
    }
    let (lo, hi) = (c - pitch / 2.0 - origin, c + pitch / 2.0 - origin);
    let mut v = Vec::new();
    let mut k = (lo / PITCH).floor() as i64;
    while (k as f64) * PITCH < hi {
        for spot in [PITCH / 2.0 - HOLE_OFFSET, PITCH / 2.0 + HOLE_OFFSET] {
            let x = k as f64 * PITCH + spot;
            if x > lo && x < hi {
                v.push(origin + x);
            }
        }
        k += 1;
    }
    v
}

/// A rectangular block of cells: its corner, cell counts and cell size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub corner: [f64; 2],
    pub nx: u32,
    pub ny: u32,
    pub pitch: [f64; 2],
}

/// Repeat the shapes drawn since the sketch began (or since the last pattern) as a grid:
/// `count` copies `spacing` apart along X and `count2` along Y, the drawn ones included. The
/// plugin turns this into `sketch.pattern.rectangular` with the ids of those shapes, so a
/// grid costs two host calls however many cells it has (a run may make only 256).
pub(crate) fn pattern(count: u32, spacing: f64, count2: u32, spacing2: f64) -> Option<Value> {
    (count * count2 > 1).then(|| cmd("@pattern", json!({"count": count, "spacing": r4(spacing), "count2": count2, "spacing2": r4(spacing2)})))
}

/// One rounded rectangle per cell of `b`, `w` x `d` with radius `r`, centred in each cell.
pub(crate) fn block_rrects(b: Block, w: f64, d: f64, r: f64) -> Vec<Value> {
    let [px, py] = b.pitch;
    let mut v = vec![rrect(b.corner[0] + px / 2.0, b.corner[1] + py / 2.0, w, d, r)];
    v.extend(pattern(b.nx, px, b.ny, py));
    v
}

/// Hole positions along one axis for `n` cells from `corner`, grouped into runs `step` apart
/// (the 42 mm grid's two magnet spots per cell are two such runs).
fn hole_runs(corner: f64, n: u32, pitch: f64, grid_origin: f64) -> (f64, Vec<(f64, u32)>) {
    let step = if pitch == PITCH || pitch == HALF_PITCH { PITCH } else { pitch };
    let mut xs: Vec<f64> = (0..n).flat_map(|i| hole_axis(corner + pitch * i as f64 + pitch / 2.0, grid_origin, pitch)).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut runs: Vec<(f64, u32)> = Vec::new();
    for x in xs {
        match runs.iter_mut().find(|(x0, k)| ((x - x0) - step * *k as f64).abs() < 1e-6) {
            Some(run) => run.1 += 1,
            None => runs.push((x, 1)),
        }
    }
    (step, runs)
}

/// Holes of diameter `dia` under every cell of `b` (see `hole_axis`), as a few patterns.
pub(crate) fn block_holes(b: Block, grid_origin: [f64; 2], dia: f64) -> Vec<Value> {
    let (sx, rx) = hole_runs(b.corner[0], b.nx, b.pitch[0], grid_origin[0]);
    let (sy, ry) = hole_runs(b.corner[1], b.ny, b.pitch[1], grid_origin[1]);
    let mut v = Vec::new();
    for &(x, kx) in &rx {
        for &(y, ky) in &ry {
            v.push(circle(x, y, dia));
            v.extend(pattern(kx, sx, ky, sy));
        }
    }
    v
}

/// Commands for one bin. Sketch names are prefixed with the body name so
/// several bins can live in one design.
///
/// The block with its lip, cavity and dividers comes first, while it is a simple shape; then
/// one foot, with its holes, is made as its own body, repeated under every cell and joined on.
pub fn bin_commands(o: &BinOptions) -> Vec<Value> {
    let b = o.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let mut out = Vec::new();
    let (a, m, c) = FOOT;
    let top = FOOT_TOP_R;
    let pitch = o.pitch;
    let [px, py] = pitch;
    let r_bottom = top - (a + c);
    // foot sizes along X and Y: top (full cell less the gap) and middle (above the bottom chamfer)
    let (sx_top, sy_top) = (px - GAP, py - GAP);
    let (sx_mid, sy_mid, r_mid) = (sx_top - 2.0 * c, sy_top - 2.0 * c, r_bottom + a);
    let foot_h = a + m + c;
    let w = px * o.nx as f64 - GAP;
    let d = py * o.ny as f64 - GAP;
    let cx = o.origin[0] + px * o.nx as f64 / 2.0;
    let cy = o.origin[1] + py * o.ny as f64 / 2.0;
    let height = UNIT_H * o.units as f64;
    let block_top = height + if o.lip { LIP.0 + LIP.1 + LIP.2 - LIP_FLAT } else { 0.0 };
    let blk = Block { corner: o.origin, nx: o.nx, ny: o.ny, pitch };

    sketch(&mut out, foot_h, &n("Body"), vec![rrect(cx, cy, w, d, top)]);
    out.push(extrude(&n("Body"), block_top - foot_h, Op::New(b), None, ""));

    // Cavity and stacking lip, cut from the top down.
    let tip = LIP.0 + LIP.2;
    let mut z;
    let hollow = !o.solid;
    if o.lip {
        z = block_top;
        let d1 = LIP.2 - LIP_FLAT;
        let inset = LIP_FLAT;
        sketch(&mut out, z, &n("Lip1"), vec![rrect(cx, cy, w - 2.0 * inset, d - 2.0 * inset, (top - inset).max(0.2))]);
        out.push(extrude(&n("Lip1"), d1, Op::Cut(b), Some(-45.0), "negative"));
        z -= d1;
        let inset = LIP.2;
        sketch(&mut out, z, &n("Lip2"), vec![rrect(cx, cy, w - 2.0 * inset, d - 2.0 * inset, top - inset)]);
        out.push(extrude(&n("Lip2"), LIP.1, Op::Cut(b), None, "negative"));
        z -= LIP.1;
        sketch(&mut out, z, &n("Lip3"), vec![rrect(cx, cy, w - 2.0 * inset, d - 2.0 * inset, top - inset)]);
        out.push(extrude(&n("Lip3"), LIP.0, Op::Cut(b), Some(-45.0), "negative"));
        z -= LIP.0;
        // Filled: the top is flat at the wall height, inside the lip.
        if hollow {
            sketch(&mut out, z, &n("Support1"), vec![rrect(cx, cy, w - 2.0 * tip, d - 2.0 * tip, top - tip)]);
            out.push(extrude(&n("Support1"), LIP_SUPPORT, Op::Cut(b), None, "negative"));
            z -= LIP_SUPPORT;
            let slope = tip - WALL;
            sketch(&mut out, z, &n("Support2"), vec![rrect(cx, cy, w - 2.0 * tip, d - 2.0 * tip, top - tip)]);
            out.push(extrude(&n("Support2"), slope, Op::Cut(b), Some(45.0), "negative"));
            z -= slope;
        }
    } else {
        z = height;
    }
    if hollow {
        sketch(&mut out, z, &n("Cavity"), vec![rrect(cx, cy, w - 2.0 * WALL, d - 2.0 * WALL, top - WALL)]);
        out.push(extrude(&n("Cavity"), z - FLOOR_Z, Op::Cut(b), None, "negative"));

        // Dividers from the floor to just under the lip support.
        let (iw, id) = (w - 2.0 * WALL, d - 2.0 * WALL);
        let (x0, y0) = (cx - iw / 2.0, cy - id / 2.0);
        for k in 1..=o.div_x {
            let x = x0 + iw * k as f64 / (o.div_x + 1) as f64 - DIVIDER / 2.0;
            out.push(cmd("solid.box", json!({"corner": [r4(x), r4(y0), FLOOR_Z], "length": DIVIDER, "width": r4(id),
                "height": r4(z - FLOOR_Z), "operation": "join", "targets": [b]})));
        }
        for k in 1..=o.div_y {
            let y = y0 + id * k as f64 / (o.div_y + 1) as f64 - DIVIDER / 2.0;
            out.push(cmd("solid.box", json!({"corner": [r4(x0), r4(y), FLOOR_Z], "length": r4(iw), "width": DIVIDER,
                "height": r4(z - FLOOR_Z), "operation": "join", "targets": [b]})));
        }
    }

    // Screw holes reach above the feet into the block: that part is cut now, while it is simple.
    if o.screws && SCREW_DEPTH > foot_h {
        sketch(&mut out, foot_h, &n("Screws above"), block_holes(blk, o.origin, SCREW_D));
        out.push(extrude(&n("Screws above"), SCREW_DEPTH - foot_h, Op::Cut(b), None, "positive"));
    }

    // One foot under the first cell, with its holes, as a body of its own.
    let foot = n("Foot");
    let one = Block { nx: 1, ny: 1, ..blk };
    sketch(&mut out, foot_h, &n("Feet3"), block_rrects(one, sx_top, sy_top, top));
    out.push(extrude(&n("Feet3"), c, Op::New(&foot), Some(-45.0), "negative"));
    sketch(&mut out, a + m, &n("Feet2"), block_rrects(one, sx_mid, sy_mid, r_mid));
    out.push(extrude(&n("Feet2"), m, Op::Join(&foot), None, "negative"));
    sketch(&mut out, a, &n("Feet1"), block_rrects(one, sx_mid, sy_mid, r_mid));
    out.push(extrude(&n("Feet1"), a, Op::Join(&foot), Some(-45.0), "negative"));
    // Holes that are the same under every cell go into the foot; the half grid's alternate
    // between cells, so they are cut after the feet are joined.
    let same_holes = pitch[0] != HALF_PITCH && pitch[1] != HALF_PITCH;
    let holes = |out: &mut Vec<Value>, target: &str, blk: Block| {
        if o.magnets {
            sketch(out, 0.0, &n("Magnets"), block_holes(blk, o.origin, MAGNET_D));
            out.push(extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(target), None, "positive"));
        }
        if o.screws {
            sketch(out, 0.0, &n("Screws"), block_holes(blk, o.origin, SCREW_D));
            out.push(extrude(&n("Screws"), SCREW_DEPTH.min(foot_h), Op::Cut(target), None, "positive"));
        }
    };
    if same_holes {
        holes(&mut out, &foot, one);
    }
    // a foot under every cell, then all of them joined to the bin at once (see plate.rs on why
    // big bodies are changed as seldom as possible)
    let many = o.nx * o.ny > 1;
    if many {
        let mut p = json!({"bodies": [foot], "dir1": [1, 0, 0], "count1": o.nx, "spacing1": r4(px)});
        if o.ny > 1 {
            p["dir2"] = json!([0, 1, 0]);
            p["count2"] = json!(o.ny);
            p["spacing2"] = json!(r4(py));
        }
        if o.nx == 1 {
            p = json!({"bodies": [foot], "dir1": [0, 1, 0], "count1": o.ny, "spacing1": r4(py)});
        }
        out.push(cmd("solid.pattern.rectangular", p));
    }
    let feet = if many { json!([foot, "@created"]) } else { json!([foot]) };
    if !same_holes {
        // the half grid's holes alternate between cells: cut into all the feet before joining
        let start = out.len();
        holes(&mut out, &foot, blk);
        for c in &mut out[start..] {
            if c["command"] == "solid.extrude" {
                c["params"]["targets"] = feet.clone();
            }
        }
    }
    out.push(cmd("solid.combine", json!({"target": b, "tools": feet, "operation": "join"})));
    out
}

pub fn baseplate_commands(o: &BaseplateOptions) -> Vec<Value> {
    let magnets = o.magnets && !o.open;
    let piece = crate::plate::Piece {
        name: o.name.clone(),
        cells: Block { corner: o.origin, nx: o.nx, ny: o.ny, pitch: o.pitch },
        h: crate::plate::plate_height(o.open, magnets),
        open: o.open,
        magnets,
        grid_origin: o.origin,
        filler: [0.0; 4],
        round: [true; 4],
        tabs: [false; 2],
        sockets: [false; 2],
        make_cell: true,
    };
    let mut out = Vec::new();
    crate::plate::piece_commands(&mut out, &piece);
    out
}

/// The commands with every `@pattern` written out as copies of the shapes it repeats: what
/// SolidCraft ends up drawing. For tests, and to compare with the Python reference.
pub fn expand(cmds: &[Value]) -> Vec<Value> {
    fn shifted(c: &Value, dx: f64, dy: f64) -> Value {
        let mut c = c.clone();
        let mv = |p: &mut Value| {
            let (x, y) = (p[0].as_f64().unwrap(), p[1].as_f64().unwrap());
            *p = json!([r4(x + dx), r4(y + dy)]);
        };
        let params = &mut c["params"];
        for k in ["p0", "p1", "center", "start", "end"] {
            if params.get(k).is_some() {
                mv(&mut params[k]);
            }
        }
        if let Some(Value::Array(pts)) = params.get_mut("points") {
            pts.iter_mut().for_each(mv);
        }
        c
    }
    let mut out = Vec::new();
    let mut since = 0;
    for c in cmds {
        match c["command"].as_str().unwrap() {
            "@pattern" => {
                let p = &c["params"];
                let shapes: Vec<Value> = out[since..].to_vec();
                let (n1, s1) = (p["count"].as_u64().unwrap(), p["spacing"].as_f64().unwrap());
                let (n2, s2) = (p["count2"].as_u64().unwrap(), p["spacing2"].as_f64().unwrap());
                for j in 0..n2 {
                    for i in 0..n1 {
                        if i + j > 0 {
                            out.extend(shapes.iter().map(|v| shifted(v, s1 * i as f64, s2 * j as f64)));
                        }
                    }
                }
                since = out.len();
            }
            "sketch.create" => {
                out.push(c.clone());
                since = out.len();
            }
            _ => out.push(c.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(nx: u32, ny: u32, units: u32) -> BinOptions {
        BinOptions { nx, ny, pitch: [PITCH; 2], units, magnets: true, screws: false, div_x: 1, div_y: 0, lip: true, solid: false,
                     origin: [0.0, 0.0], name: "Bin".into() }
    }

    fn circles(c: &[Value]) -> Vec<(f64, f64)> {
        let mut v: Vec<(f64, f64)> = c.iter().filter(|v| v["command"] == "sketch.circle.center")
            .map(|v| (v["params"]["center"][0].as_f64().unwrap(), v["params"]["center"][1].as_f64().unwrap())).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }

    fn sketches(c: &[Value]) -> Vec<String> {
        c.iter().filter(|v| v["command"] == "sketch.create").map(|v| v["params"]["name"].as_str().unwrap().to_string()).collect()
    }

    fn body_pattern(c: &[Value]) -> Option<(u64, u64)> {
        c.iter().find(|v| v["command"] == "solid.pattern.rectangular").map(|v| {
            let p = &v["params"];
            (p["count1"].as_u64().unwrap(), p["count2"].as_u64().unwrap_or(1))
        })
    }

    // The shapes themselves were checked in SolidCraft: the volumes in gen/volumes.json match
    // the reference generator's to the micrometre. These tests pin the way they are built.

    #[test]
    fn bin_is_block_then_one_foot_repeated() {
        let c = bin_commands(&bin(2, 1, 3));
        // body block from the top of the feet to 7u + lip (minus the flat)
        assert_eq!(c[0]["params"]["offset"], json!(4.75));
        assert_eq!(c[3]["params"]["distance"], json!(r4(21.0 + 4.4 - 0.3 - 4.75)));
        assert!(c.iter().any(|v| v["command"] == "solid.box"));
        // one foot, 41.5 square at the top, made as its own body with its four magnet holes
        let feet3 = c.iter().position(|v| v["params"]["name"] == "Bin Feet3").unwrap();
        assert_eq!(c[feet3 + 1]["params"]["p1"], json!([41.75, 41.75]));
        assert!(c.iter().any(|v| v["command"] == "solid.extrude" && v["params"]["body_name"] == "Bin Foot"));
        assert_eq!(circles(&c).len(), 4);
        // repeated 2 x 1 and all joined into the bin at once
        assert_eq!(body_pattern(&c), Some((2, 1)));
        let joins: Vec<_> = c.iter().filter(|v| v["command"] == "solid.combine").map(|v| v["params"].clone()).collect();
        assert_eq!(joins, vec![json!({"target": "Bin", "tools": ["Bin Foot", "@created"], "operation": "join"})]);
        // the cavity is cut before the foot exists
        let names = sketches(&c);
        let (cav, foot) = (names.iter().position(|n| n == "Bin Cavity").unwrap(), names.iter().position(|n| n == "Bin Feet3").unwrap());
        assert!(cav < foot);
    }

    #[test]
    fn half_grid_bin_has_one_magnet_per_foot_on_the_baseplate_spots() {
        let mut o = bin(2, 2, 3);
        o.pitch = [HALF_PITCH; 2];
        let c = bin_commands(&o);
        // the holes alternate between half cells, so they are cut into all the feet once they
        // are repeated, before they are joined
        let pat = c.iter().position(|v| v["command"] == "solid.pattern.rectangular").unwrap();
        let join = c.iter().position(|v| v["command"] == "solid.combine").unwrap();
        let magnets = c.iter().position(|v| v["params"]["name"] == "Bin Magnets").unwrap();
        assert!(pat < magnets && magnets < join);
        let cut = c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"] == "Bin Magnets").unwrap();
        assert_eq!(cut["params"]["targets"], json!(["Bin Foot", "@created"]));
        // a 2x2 half bin covers one 42 mm cell: the same four spots as a full 1x1 bin
        assert_eq!(circles(&expand(&c)), vec![(8.0, 8.0), (8.0, 34.0), (34.0, 8.0), (34.0, 34.0)]);
        // 41.5 mm outside, feet 20.5 mm at the top
        assert!(c.iter().any(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["p1"] == json!([41.75, 41.75])));
        assert!(c.iter().any(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["p1"] == json!([20.75, 20.75])));
    }

    #[test]
    fn solid_bin_has_no_cavity_or_dividers_but_keeps_lip_and_holes() {
        let mut o = bin(2, 1, 3);
        o.solid = true;
        let c = bin_commands(&o);
        let names = sketches(&c);
        assert!(names.iter().any(|n| n.ends_with("Lip3")));
        assert!(!names.iter().any(|n| n.ends_with("Cavity") || n.ends_with("Support1")));
        assert!(!c.iter().any(|v| v["command"] == "solid.box"));
        assert_eq!(circles(&c).len(), 4);
        o.lip = false;
        let cuts: Vec<_> = bin_commands(&o).into_iter().filter(|v| v["command"] == "solid.extrude" && v["params"]["operation"] == "cut").collect();
        assert!(cuts.iter().all(|v| v["params"]["sketch"].as_str().unwrap().ends_with("Magnets")));
    }

    #[test]
    fn screw_holes_are_cut_in_two_parts() {
        let mut o = bin(1, 1, 3);
        o.screws = true;
        let c = bin_commands(&o);
        let depth = |name: &str| c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"] == name).unwrap()["params"]["distance"].as_f64().unwrap();
        // 4.75 in the foot and 1.25 in the block above it: 6 mm, as before
        assert!((depth("Bin Screws") + depth("Bin Screws above") - SCREW_DEPTH).abs() < 1e-9);
    }

    #[test]
    fn full_grid_holes_unchanged() {
        assert_eq!(hole_axis(21.0, 0.0, PITCH), vec![8.0, 34.0]);
        assert_eq!(hole_axis(10.5, 0.0, HALF_PITCH), vec![8.0]);
        assert_eq!(hole_axis(31.5, 0.0, HALF_PITCH), vec![34.0]);
        assert_eq!(hole_axis(52.5, 0.0, HALF_PITCH), vec![50.0]);
    }

    #[test]
    fn holes_over_many_cells_are_a_few_patterns() {
        // 12 x 12 full cells: 576 magnet holes, drawn as 4 circles and 4 patterns
        let b = Block { corner: [0.0, 0.0], nx: 12, ny: 12, pitch: [PITCH; 2] };
        let h = block_holes(b, [0.0, 0.0], MAGNET_D);
        assert_eq!(h.len(), 8);
        assert_eq!(circles(&expand(&[vec![cmd("sketch.create", json!({}))], h].concat())).len(), 576);
        // half grid: one hole per cell, still on the 42 mm grid's spots
        let b = Block { corner: [0.0, 0.0], nx: 3, ny: 1, pitch: [HALF_PITCH; 2] };
        let got = circles(&expand(&[vec![cmd("sketch.create", json!({}))], block_holes(b, [0.0, 0.0], MAGNET_D)].concat()));
        assert_eq!(got, vec![(8.0, 8.0), (34.0, 8.0), (50.0, 8.0)]);
    }

    #[test]
    fn custom_size_bin_and_baseplate_agree() {
        // 40 x 25 mm cells: two holes 8 mm in from the edges along X, one in the middle along Y
        let mut o = bin(1, 1, 3);
        o.pitch = [40.0, 25.0];
        let c = bin_commands(&o);
        assert_eq!(circles(&c), vec![(8.0, 12.5), (32.0, 12.5)]);
        // outside 39.5 x 24.5, feet the same at the top
        assert!(c.iter().any(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["p1"] == json!([39.75, 24.75])));
        let p = BaseplateOptions { nx: 1, ny: 1, pitch: [40.0, 25.0], open: false, magnets: true, origin: [0.0, 0.0], name: "Plate".into() };
        let pc = baseplate_commands(&p);
        let magnets = |c: &[Value]| -> Vec<Value> { c.iter().filter(|v| v["params"]["diameter"] == json!(MAGNET_D)).cloned().collect() };
        assert_eq!(circles(&magnets(&pc)), circles(&c));
        // a 42 mm "custom" size is just the full grid
        assert_eq!(hole_axis(21.0, 0.0, 42.0), vec![8.0, 34.0]);
        assert_eq!(hole_axis(15.0, 0.0, 30.0), vec![8.0, 22.0]);
        assert_eq!(hole_axis(7.5, 0.0, 15.0), vec![7.5]);
    }

    #[test]
    fn baseplate_is_one_cell_repeated_with_rounded_corners() {
        let c = baseplate_commands(&BaseplateOptions { nx: 3, ny: 2, pitch: [PITCH; 2], open: false, magnets: false, origin: [0.0, 0.0], name: "Plate".into() });
        let pockets = c.iter().filter(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["radius"] == json!(3.95)).count();
        assert_eq!(pockets, 1);
        // a row of 3, then 2 rows
        let pats: Vec<_> = c.iter().filter(|v| v["command"] == "solid.pattern.rectangular").map(|v| v["params"]["count1"].clone()).collect();
        assert_eq!(pats, vec![json!(3), json!(2)]);
        // two corners cut on the front row, two on the back row, before the rows are joined (once)
        let cut = |name: &str| c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"] == name).unwrap().clone();
        assert_eq!(cut("Plate Corners front")["params"]["profiles"].as_array().unwrap().len(), 2);
        assert_eq!(cut("Plate Corners back")["params"]["targets"], json!(["@last"]));
        let joins: Vec<_> = c.iter().enumerate().filter(|(_, v)| v["command"] == "solid.combine").map(|(i, _)| i).collect();
        assert_eq!(joins.len(), 2);
        assert_eq!(*joins.last().unwrap(), c.len() - 1);
    }

    #[test]
    fn the_biggest_sizes_stay_within_256_calls() {
        let mut o = bin(24, 24, 20);
        o.pitch = [HALF_PITCH; 2];
        o.screws = true;
        o.div_x = 10;
        o.div_y = 10;
        assert!(bin_commands(&o).len() < 100);
        let p = BaseplateOptions { nx: 24, ny: 24, pitch: [HALF_PITCH; 2], open: false, magnets: true, origin: [0.0, 0.0], name: "P".into() };
        assert!(baseplate_commands(&p).len() < 100);
    }
}
