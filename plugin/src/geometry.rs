//! Gridfinity bins and baseplates as a list of SolidCraft commands.
//!
//! A direct port of `gen/gridfinity.py`, whose output was checked against the
//! spec in research/SPEC.md by measuring the exported STL. Keep the two in step.

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
const MAGNET_D: f64 = 6.5;
const MAGNET_DEPTH: f64 = 2.4;
const SCREW_D: f64 = 3.0;
const SCREW_DEPTH: f64 = 6.0;
const PLATE_PROFILE: (f64, f64, f64) = (0.7, 1.8, 2.15);
const PLATE_R: f64 = 4.0;
const PLATE_RIM: f64 = 0.05;

#[derive(Debug, Clone, PartialEq)]
pub struct BinOptions {
    pub nx: u32,
    pub ny: u32,
    /// Cell size in mm: PITCH (full grid) or HALF_PITCH.
    pub pitch: f64,
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
    /// Pocket size in mm: PITCH (full grid) or HALF_PITCH.
    pub pitch: f64,
    /// Frame only: the pockets go right through (no floor, so no magnet holes).
    pub open: bool,
    pub magnets: bool,
    pub origin: [f64; 2],
    pub name: String,
}

fn r4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn cmd(command: &str, params: Value) -> Value {
    json!({"command": command, "params": params})
}

fn rrect(cx: f64, cy: f64, w: f64, d: f64, r: f64) -> Value {
    cmd(
        "sketch.rectangle.rounded",
        json!({"p0": [r4(cx - w / 2.0), r4(cy - d / 2.0)], "p1": [r4(cx + w / 2.0), r4(cy + d / 2.0)], "radius": r4(r)}),
    )
}

fn circle(x: f64, y: f64, d: f64) -> Value {
    cmd("sketch.circle.center", json!({"center": [r4(x), r4(y)], "diameter": d}))
}

fn sketch(out: &mut Vec<Value>, z: f64, name: &str, items: Vec<Value>) {
    out.push(cmd("sketch.create", json!({"plane": "XY", "offset": r4(z), "name": name})));
    out.extend(items);
    out.push(cmd("sketch.finish", json!({})));
}

enum Op<'a> {
    New(&'a str),
    Join(&'a str),
    Cut(&'a str),
}

/// `dir`: "negative" (down), "positive", or "" for the default.
fn extrude(sketch: &str, distance: f64, op: Op, taper: Option<f64>, dir: &str) -> Value {
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

fn cells(nx: u32, ny: u32, origin: [f64; 2], pitch: f64) -> Vec<(f64, f64)> {
    let mut v = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            v.push((origin[0] + pitch * i as f64 + pitch / 2.0, origin[1] + pitch * j as f64 + pitch / 2.0));
        }
    }
    v
}

/// Hole positions along one axis inside a foot centred at `c` (width `pitch`): the 42 mm grid's
/// magnet spots, ±13 mm from each 42 mm cell centre counted from `origin`. A full foot gets two
/// per axis (four holes); a half-grid foot gets the one that lines up with a baseplate magnet.
fn hole_axis(c: f64, origin: f64, pitch: f64) -> Vec<f64> {
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

fn hole_circles(cells: &[(f64, f64)], origin: [f64; 2], pitch: f64, dia: f64) -> Vec<Value> {
    let mut v = Vec::new();
    for &(x, y) in cells {
        for hx in hole_axis(x, origin[0], pitch) {
            for hy in hole_axis(y, origin[1], pitch) {
                v.push(circle(hx, hy, dia));
            }
        }
    }
    v
}

/// Commands for one bin. Sketch names are prefixed with the body name so
/// several bins can live in one design.
pub fn bin_commands(o: &BinOptions) -> Vec<Value> {
    let b = o.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let mut out = Vec::new();
    let (a, m, c) = FOOT;
    let top = FOOT_TOP_R;
    let pitch = o.pitch;
    let s_bottom = (pitch - GAP) - 2.0 * (a + c);
    let r_bottom = top - (a + c);
    let (s_mid, r_mid) = (s_bottom + 2.0 * a, r_bottom + a);
    let foot_h = a + m + c;
    let w = pitch * o.nx as f64 - GAP;
    let d = pitch * o.ny as f64 - GAP;
    let cx = o.origin[0] + pitch * o.nx as f64 / 2.0;
    let cy = o.origin[1] + pitch * o.ny as f64 / 2.0;
    let height = UNIT_H * o.units as f64;
    let block_top = height + if o.lip { LIP.0 + LIP.1 + LIP.2 - LIP_FLAT } else { 0.0 };
    let cl = cells(o.nx, o.ny, o.origin, pitch);

    // Body first, then the feet downwards from it, so everything is one body.
    sketch(&mut out, foot_h, &n("Body"), vec![rrect(cx, cy, w, d, top)]);
    out.push(extrude(&n("Body"), block_top - foot_h, Op::New(b), None, ""));
    let s_top = pitch - GAP;
    sketch(&mut out, foot_h, &n("Feet3"), cl.iter().map(|&(x, y)| rrect(x, y, s_top, s_top, top)).collect());
    out.push(extrude(&n("Feet3"), c, Op::Join(b), Some(-45.0), "negative"));
    sketch(&mut out, a + m, &n("Feet2"), cl.iter().map(|&(x, y)| rrect(x, y, s_mid, s_mid, r_mid)).collect());
    out.push(extrude(&n("Feet2"), m, Op::Join(b), None, "negative"));
    sketch(&mut out, a, &n("Feet1"), cl.iter().map(|&(x, y)| rrect(x, y, s_mid, s_mid, r_mid)).collect());
    out.push(extrude(&n("Feet1"), a, Op::Join(b), Some(-45.0), "negative"));

    // Cavity and stacking lip, cut from the top down.
    let tip = LIP.0 + LIP.2;
    let mut z;
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
        if o.solid {
            // Filled: the top is flat at the wall height, inside the lip.
            return holes(out, o, &n, &cl);
        }
        sketch(&mut out, z, &n("Support1"), vec![rrect(cx, cy, w - 2.0 * tip, d - 2.0 * tip, top - tip)]);
        out.push(extrude(&n("Support1"), LIP_SUPPORT, Op::Cut(b), None, "negative"));
        z -= LIP_SUPPORT;
        let slope = tip - WALL;
        sketch(&mut out, z, &n("Support2"), vec![rrect(cx, cy, w - 2.0 * tip, d - 2.0 * tip, top - tip)]);
        out.push(extrude(&n("Support2"), slope, Op::Cut(b), Some(45.0), "negative"));
        z -= slope;
    } else {
        z = height;
        if o.solid {
            return holes(out, o, &n, &cl);
        }
    }
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

    holes(out, o, &n, &cl)
}

/// Magnet and screw holes in the bottom of every foot.
fn holes(mut out: Vec<Value>, o: &BinOptions, n: &dyn Fn(&str) -> String, cl: &[(f64, f64)]) -> Vec<Value> {
    let b = o.name.as_str();
    let pitch = o.pitch;
    // Holes in the bottom of every foot.
    if o.magnets {
        sketch(&mut out, 0.0, &n("Magnets"), hole_circles(cl, o.origin, pitch, MAGNET_D));
        out.push(extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(b), None, "positive"));
    }
    if o.screws {
        sketch(&mut out, 0.0, &n("Screws"), hole_circles(cl, o.origin, pitch, SCREW_D));
        out.push(extrude(&n("Screws"), SCREW_DEPTH, Op::Cut(b), None, "positive"));
    }
    out
}

pub fn baseplate_commands(o: &BaseplateOptions) -> Vec<Value> {
    let b = o.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let mut out = Vec::new();
    let (a, m, c) = PLATE_PROFILE;
    let depth = a + m + c;
    let rim = PLATE_RIM;
    let pitch = o.pitch;
    let magnets = o.magnets && !o.open;
    // An open frame is just the pocket profile high (the pockets end at its underside).
    let h = if o.open { depth - rim } else if magnets { depth + MAGNET_DEPTH + 0.6 } else { depth + 0.35 };
    let (w, d) = (pitch * o.nx as f64, pitch * o.ny as f64);
    let (cx, cy) = (o.origin[0] + w / 2.0, o.origin[1] + d / 2.0);
    let cl = cells(o.nx, o.ny, o.origin, pitch);
    sketch(&mut out, 0.0, &n("Plate"), vec![rrect(cx, cy, w, d, PLATE_R)]);
    out.push(extrude(&n("Plate"), h, Op::New(b), None, ""));
    // The spec's pockets meet at knife edges; the kernel can't taper profiles that
    // touch, so each pocket keeps a 0.05 mm rim and its 45° starts 0.05 lower.
    let (s_top, r_top) = (pitch - 2.0 * rim, PLATE_R - rim);
    let (s_mid, r_mid) = (pitch - 2.0 * c, PLATE_R - c);
    sketch(&mut out, h, &n("Pocket1"), cl.iter().map(|&(x, y)| rrect(x, y, s_top, s_top, r_top)).collect());
    out.push(extrude(&n("Pocket1"), c - rim, Op::Cut(b), Some(-45.0), "negative"));
    sketch(&mut out, h - c + rim, &n("Pocket2"), cl.iter().map(|&(x, y)| rrect(x, y, s_mid, s_mid, r_mid)).collect());
    out.push(extrude(&n("Pocket2"), m, Op::Cut(b), None, "negative"));
    sketch(&mut out, h - c + rim - m, &n("Pocket3"), cl.iter().map(|&(x, y)| rrect(x, y, s_mid, s_mid, r_mid)).collect());
    // An open frame's last cut runs a little past the underside so it goes right through.
    let last = if o.open { a + 0.2 } else { a };
    out.push(extrude(&n("Pocket3"), last, Op::Cut(b), Some(-45.0), "negative"));
    if magnets {
        let floor = h - (depth - rim);
        sketch(&mut out, floor, &n("Magnets"), hole_circles(&cl, o.origin, pitch, MAGNET_D));
        out.push(extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(b), None, "negative"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin(nx: u32, ny: u32, units: u32) -> BinOptions {
        BinOptions { nx, ny, pitch: PITCH, units, magnets: true, screws: false, div_x: 1, div_y: 0, lip: true, solid: false,
                     origin: [0.0, 0.0], name: "Bin".into() }
    }

    #[test]
    fn bin_matches_the_python_reference_shape() {
        let c = bin_commands(&bin(2, 1, 3));
        // body block from the top of the feet to 7u + lip (minus the flat)
        assert_eq!(c[0]["params"]["offset"], json!(4.75));
        assert_eq!(c[3]["params"]["distance"], json!(r4(21.0 + 4.4 - 0.3 - 4.75)));
        // two feet per sketch, 41.5 square at the top
        let feet: Vec<_> = c.iter().filter(|v| v["command"] == "sketch.rectangle.rounded"
            && v["params"]["radius"] == json!(3.75) && v["params"]["p1"][0] == json!(41.75)).collect();
        assert_eq!(feet.len(), 1);
        assert!(c.iter().any(|v| v["command"] == "solid.box"));
        let magnets = c.iter().filter(|v| v["command"] == "sketch.circle.center").count();
        assert_eq!(magnets, 8);
    }

    /// Same commands as the Python reference (whose STL was measured), apart from the
    /// body-name prefix on sketch names.
    fn same_as_reference(mut rust: Vec<Value>, reference: &str, prefix: &str) {
        for c in rust.iter_mut() {
            for key in ["name", "sketch"] {
                if let Some(Value::String(s)) = c["params"].get_mut(key) {
                    *s = s.trim_start_matches(prefix).to_string();
                }
            }
        }
        // 0 and 0.0 differ as JSON values but not as numbers: compare as f64.
        fn norm(v: &Value) -> Value {
            match v {
                Value::Number(n) => json!(n.as_f64().unwrap()),
                Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
                Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), norm(v))).collect()),
                other => other.clone(),
            }
        }
        let py: Value = serde_json::from_str(reference).unwrap();
        let (rust, py) = (norm(&Value::Array(rust)), norm(&py["commands"]));
        let (ra, pa) = (rust.as_array().unwrap(), py.as_array().unwrap());
        for (i, (r, p)) in ra.iter().zip(pa).enumerate() {
            assert_eq!(r, p, "command {i} differs");
        }
        assert_eq!(ra.len(), pa.len());
    }

    #[test]
    fn bin_equals_python_reference() {
        same_as_reference(bin_commands(&bin(2, 1, 3)), include_str!("../../gen/reference/bin-2x1x3.json"), "Bin ");
    }

    #[test]
    fn baseplate_equals_python_reference() {
        let o = BaseplateOptions { nx: 3, ny: 2, pitch: PITCH, open: false, magnets: true, origin: [0.0, 0.0], name: "Baseplate".into() };
        same_as_reference(baseplate_commands(&o), include_str!("../../gen/reference/baseplate-3x2.json"), "Baseplate ");
    }

    #[test]
    fn half_grid_bin_has_one_magnet_per_foot_on_the_baseplate_spots() {
        let mut o = bin(2, 2, 3);
        o.pitch = HALF_PITCH;
        let c = bin_commands(&o);
        let holes: Vec<(f64, f64)> = c.iter().filter(|v| v["command"] == "sketch.circle.center")
            .map(|v| (v["params"]["center"][0].as_f64().unwrap(), v["params"]["center"][1].as_f64().unwrap())).collect();
        // a 2x2 half bin covers one 42 mm cell: the same four spots as a full 1x1 bin
        let mut want = vec![(8.0, 8.0), (8.0, 34.0), (34.0, 8.0), (34.0, 34.0)];
        let mut got = holes.clone();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, want);
        // 41.5 mm outside, feet 20.5 mm at the top
        assert!(c.iter().any(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["p1"] == json!([41.75, 41.75])));
        assert!(c.iter().any(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["p1"] == json!([20.75, 20.75])));
    }

    #[test]
    fn solid_bin_has_no_cavity_or_dividers_but_keeps_lip_and_holes() {
        let mut o = bin(2, 1, 3);
        o.solid = true;
        let c = bin_commands(&o);
        let names: Vec<String> = c.iter().filter(|v| v["command"] == "sketch.create")
            .map(|v| v["params"]["name"].as_str().unwrap().to_string()).collect();
        assert!(names.iter().any(|n| n.ends_with("Lip3")));
        assert!(!names.iter().any(|n| n.ends_with("Cavity") || n.ends_with("Support1")));
        assert!(!c.iter().any(|v| v["command"] == "solid.box"));
        assert_eq!(c.iter().filter(|v| v["command"] == "sketch.circle.center").count(), 8);
        o.lip = false;
        assert!(!bin_commands(&o).iter().any(|v| v["command"] == "solid.extrude" && v["params"]["operation"] == "cut"
            && !v["params"]["sketch"].as_str().unwrap().ends_with("Magnets")));
    }

    #[test]
    fn full_grid_holes_unchanged() {
        assert_eq!(hole_axis(21.0, 0.0, PITCH), vec![8.0, 34.0]);
        assert_eq!(hole_axis(10.5, 0.0, HALF_PITCH), vec![8.0]);
        assert_eq!(hole_axis(31.5, 0.0, HALF_PITCH), vec![34.0]);
        assert_eq!(hole_axis(52.5, 0.0, HALF_PITCH), vec![50.0]);
    }

    #[test]
    fn baseplate_has_one_pocket_per_cell() {
        let c = baseplate_commands(&BaseplateOptions { nx: 3, ny: 2, pitch: PITCH, open: false, magnets: false, origin: [0.0, 0.0], name: "Plate".into() });
        let pockets = c.iter().filter(|v| v["command"] == "sketch.rectangle.rounded" && v["params"]["radius"] == json!(3.95)).count();
        assert_eq!(pockets, 6);
    }
}
