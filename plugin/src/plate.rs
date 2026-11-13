//! Baseplates, built so they stay quick in a plugin run.
//!
//! In a plugin run SolidCraft spends time after every command in proportion to how big the
//! bodies it changed are (a command on a 6 x 6 plate costs 8 to 10 s, on a single row well under
//! one), and the whole run must finish in 30 s. So a plate is built from small bodies: one cell,
//! copied into a row; the row gets what every row has (filler at its ends, the tabs and sockets
//! of the seams across X); the row is copied for every row; the first and last rows get what only
//! they have (filler, rounded corners, the tabs and sockets of the seams across Y); and only then
//! are the rows joined, once.

use crate::geometry::{
    block_holes, block_rrects, circle, cmd, extrude, pattern, r4, sketch, Block, Op, MAGNET_D, MAGNET_DEPTH,
    PLATE_PROFILE, PLATE_R, PLATE_RIM,
};
use serde_json::{json, Value};

/// How far a tab reaches into the neighbour, from the seam (mm). The wall beside a seam is
/// 2.15 mm wide half-way up the pocket, so tab plus clearance stays inside it.
pub const TAB_DEPTH: f64 = 1.8;
/// Half-width of a tab at the seam and at its far end: the far end is wider, so it locks.
pub const TAB_NECK: f64 = 3.0;
pub const TAB_HEAD: f64 = 4.5;
/// Gap left around a tab in the socket it fits into (mm, each side, and above it).
pub const CLEARANCE: f64 = 0.15;

/// Height of a baseplate: an open frame is just the pocket profile high (the pockets end at its
/// underside); a solid one keeps a floor, thicker with magnet holes.
pub fn plate_height(open: bool, magnets: bool) -> f64 {
    let (a, m, c) = PLATE_PROFILE;
    let depth = a + m + c;
    if open { depth - PLATE_RIM } else if magnets { depth + MAGNET_DEPTH + 0.6 } else { depth + 0.35 }
}

/// Tabs and sockets reach only as high as the straight part of the wall between pockets, so a
/// tab never shows in the pocket's top chamfer. The socket is a little taller and closed above:
/// the piece with the sockets goes on from above.
pub fn tab_height(h: f64) -> f64 {
    let (_, _, c) = PLATE_PROFILE;
    h - c + PLATE_RIM
}

/// Points of a dovetail tab (or, with `grow`, its socket) on a seam at `s` along `axis`
/// (0: a seam across X, the tab reaching +X; 1: across Y, reaching +Y), centred at `c`.
pub fn dovetail(axis: usize, s: f64, c: f64, grow: f64) -> Vec<[f64; 2]> {
    let slope = (TAB_HEAD - TAB_NECK) / TAB_DEPTH;
    let widen = grow * (1.0 + slope * slope).sqrt();
    // a tab starts 0.5 mm back inside the piece it belongs to, so it joins solidly; a socket
    // starts at the seam, so cutting it leaves the other piece alone
    let (u0, u1) = (if grow > 0.0 { s } else { s - 0.5 }, s + TAB_DEPTH + grow);
    let half = |u: f64| TAB_NECK + slope * (u - s) + widen;
    let pts = [[u0, c - half(u0)], [u1, c - half(u1)], [u1, c + half(u1)], [u0, c + half(u0)]];
    pts.iter().map(|&[u, v]| if axis == 0 { [r4(u), r4(v)] } else { [r4(v), r4(u)] }).collect()
}

fn polygon(pts: &[[f64; 2]]) -> Value {
    cmd("sketch.line", json!({"points": pts, "closed": true}))
}

/// One baseplate piece. Coordinates are where the piece is built.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub name: String,
    /// Its cells: corner of the first, counts and size.
    pub cells: Block,
    pub h: f64,
    pub open: bool,
    /// Magnet holes, and the corner of the grid they line up with.
    pub magnets: bool,
    pub grid_origin: [f64; 2],
    /// Solid filler beside the cells: left, right, front, back (mm).
    pub filler: [f64; 4],
    /// Rounded corners: front left, front right, back right, back left.
    pub round: [bool; 4],
    /// Tabs on the right and back seams, sockets on the left and front ones.
    pub tabs: [bool; 2],
    pub sockets: [bool; 2],
    /// Make the first cell (else a body of this name already holds it, see `first_cell`).
    pub make_cell: bool,
}

/// Commands for one piece; see the module comment for the order.
pub fn piece_commands(out: &mut Vec<Value>, p: &Piece) {
    let b = p.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let Block { corner: [x0, y0], nx, ny, pitch: [px, py] } = p.cells;
    let [fl, fr, ff, fb] = p.filler;
    let h = p.h;
    let half = px == crate::geometry::HALF_PITCH || py == crate::geometry::HALF_PITCH;

    if p.make_cell {
        first_cell(out, p);
    }
    // the first row
    if nx > 1 {
        out.push(cmd("solid.pattern.rectangular", json!({"bodies": [b], "dir1": [1, 0, 0], "count1": nx, "spacing1": r4(px)})));
        out.push(cmd("solid.combine", json!({"target": b, "tools": ["@created"], "operation": "join"})));
    }
    // what every row has: filler at its ends, the seams across X
    let x1 = x0 + px * nx as f64;
    let mut boxes = Vec::new();
    if fl > 1e-6 {
        boxes.push([x0 - fl, y0, fl, py]);
    }
    if fr > 1e-6 {
        boxes.push([x1, y0, fr, py]);
    }
    for [x, y, w, d] in boxes {
        out.push(cmd("solid.box", json!({"corner": [r4(x), r4(y), 0.0], "length": r4(w), "width": r4(d), "height": r4(h),
            "operation": "join", "targets": [b]})));
    }
    let tab_h = tab_height(h);
    let row_c = y0 + py / 2.0;
    if p.sockets[0] {
        sketch(out, 0.0, &n("Sockets X"), vec![polygon(&dovetail(0, x0 - fl, row_c, CLEARANCE))]);
        out.push(extrude(&n("Sockets X"), tab_h + CLEARANCE, Op::Cut(b), None, ""));
    }
    if p.tabs[0] {
        sketch(out, 0.0, &n("Tabs X"), vec![polygon(&dovetail(0, x1 + fr, row_c, 0.0))]);
        out.push(extrude(&n("Tabs X"), tab_h, Op::Join(b), None, ""));
    }
    // every row; then the first (front) and last (back) rows get their own things
    let last = if ny > 1 {
        out.push(cmd("solid.pattern.rectangular", json!({"bodies": [b], "dir1": [0, 1, 0], "count1": ny, "spacing1": r4(py)})));
        "@last"
    } else {
        b
    };
    let (left, right, front, back) = (x0 - fl, x1 + fr, y0, y0 + py * ny as f64);
    let col_c = x0 + px / 2.0;
    // front row
    if ff > 1e-6 {
        out.push(cmd("solid.box", json!({"corner": [r4(left), r4(front - ff), 0.0], "length": r4(right - left), "width": r4(ff),
            "height": r4(h), "operation": "join", "targets": [b]})));
    }
    let front = front - ff;
    let back = back + fb;
    corners(out, &n("Corners front"), b, h, &[(p.round[0], [left, front], [1.0, 1.0]), (p.round[1], [right, front], [-1.0, 1.0])]);
    if p.sockets[1] {
        let mut v = vec![polygon(&dovetail(1, front, col_c, CLEARANCE))];
        v.extend(pattern(nx, px, 1, 0.0));
        sketch(out, 0.0, &n("Sockets Y"), v);
        out.push(extrude(&n("Sockets Y"), tab_h + CLEARANCE, Op::Cut(b), None, ""));
    }
    // back row (the same body when there is one row)
    if fb > 1e-6 {
        out.push(cmd("solid.box", json!({"corner": [r4(left), r4(back - fb), 0.0], "length": r4(right - left), "width": r4(fb),
            "height": r4(h), "operation": "join", "targets": [last]})));
    }
    corners(out, &n("Corners back"), last, h, &[(p.round[2], [right, back], [-1.0, -1.0]), (p.round[3], [left, back], [1.0, -1.0])]);
    if p.tabs[1] {
        let mut v = vec![polygon(&dovetail(1, back, col_c, 0.0))];
        v.extend(pattern(nx, px, 1, 0.0));
        sketch(out, 0.0, &n("Tabs Y"), v);
        out.push(extrude(&n("Tabs Y"), tab_h, Op::Join(last), None, ""));
    }
    // half-grid magnets differ from row to row, so they go into each row now
    if p.magnets && half {
        let rows: Vec<&str> = if ny > 1 { vec![b, "@created"] } else { vec![b] };
        let floor = h - (PLATE_PROFILE.0 + PLATE_PROFILE.1 + PLATE_PROFILE.2 - PLATE_RIM);
        sketch(out, floor, &n("Magnets"), block_holes(p.cells, p.grid_origin, MAGNET_D));
        let mut e = extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(b), None, "negative");
        e["params"]["targets"] = json!(rows);
        out.push(e);
    }
    // and only now one plate
    if ny > 1 {
        out.push(cmd("solid.combine", json!({"target": b, "tools": ["@created"], "operation": "join"})));
    }
}

/// The first cell of `p`, as body `p.name`: a square block with its pocket and, when they are
/// the same in every cell, its magnet holes.
pub fn first_cell(out: &mut Vec<Value>, p: &Piece) {
    let b = p.name.as_str();
    let n = |s: &str| format!("{b} {s}");
    let corner = p.cells.corner;
    let [px, py] = p.cells.pitch;
    let (a, m, c) = PLATE_PROFILE;
    let rim = PLATE_RIM;
    let h = p.h;
    let one = Block { corner, nx: 1, ny: 1, pitch: p.cells.pitch };
    sketch(out, 0.0, &n("Cell"), vec![cmd("sketch.rectangle.two_point",
        json!({"p0": [r4(corner[0]), r4(corner[1])], "p1": [r4(corner[0] + px), r4(corner[1] + py)]}))]);
    out.push(extrude(&n("Cell"), h, Op::New(b), None, ""));
    // The spec's pockets meet at knife edges; the kernel can't taper profiles that
    // touch, so each pocket keeps a 0.05 mm rim and its 45° starts 0.05 lower.
    let (r_top, r_mid) = (PLATE_R - rim, PLATE_R - c);
    sketch(out, h, &n("Pocket1"), block_rrects(one, px - 2.0 * rim, py - 2.0 * rim, r_top));
    out.push(extrude(&n("Pocket1"), c - rim, Op::Cut(b), Some(-45.0), "negative"));
    sketch(out, h - c + rim, &n("Pocket2"), block_rrects(one, px - 2.0 * c, py - 2.0 * c, r_mid));
    out.push(extrude(&n("Pocket2"), m, Op::Cut(b), None, "negative"));
    sketch(out, h - c + rim - m, &n("Pocket3"), block_rrects(one, px - 2.0 * c, py - 2.0 * c, r_mid));
    // An open frame's last cut runs a little past the underside so it goes right through.
    let last = if p.open { a + 0.2 } else { a };
    out.push(extrude(&n("Pocket3"), last, Op::Cut(b), Some(-45.0), "negative"));
    let half = px == crate::geometry::HALF_PITCH || py == crate::geometry::HALF_PITCH;
    if p.magnets && !half {
        let floor = h - (a + m + c - rim);
        sketch(out, floor, &n("Magnets"), block_holes(one, p.grid_origin, MAGNET_D));
        out.push(extrude(&n("Magnets"), MAGNET_DEPTH, Op::Cut(b), None, "negative"));
    }
}

/// Round corners of `body`: cut away what lies outside a `PLATE_R` arc at each corner given
/// (whether to, its point, and which way is inside along X and Y).
fn corners(out: &mut Vec<Value>, name: &str, body: &str, h: f64, list: &[(bool, [f64; 2], [f64; 2])]) {
    let r = PLATE_R;
    let mut items = Vec::new();
    let mut picks = Vec::new();
    for &(on, [x, y], [sx, sy]) in list {
        if !on {
            continue;
        }
        items.push(cmd("sketch.rectangle.two_point", json!({"p0": [r4(x), r4(y)], "p1": [r4(x + sx * r), r4(y + sy * r)]})));
        items.push(circle(x + sx * r, y + sy * r, 2.0 * r));
        // a point in the corner, outside the arc
        picks.push(json!({"point": [r4(x + sx * 0.3), r4(y + sy * 0.3)]}));
    }
    if items.is_empty() {
        return;
    }
    sketch(out, 0.0, name, items);
    let mut e = extrude(name, h, Op::Cut(body), None, "");
    e["params"]["profiles"] = json!(picks);
    out.push(e);
}
