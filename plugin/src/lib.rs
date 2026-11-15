//! Gridfinity plugin for SolidCraft: entries `bin` and `baseplate` build
//! ordinary, editable SolidCraft features through the host's command API.

pub mod drawer;
pub mod geometry;
pub mod plate;

use drawer::{drawer_commands, DrawerOptions, Filler};
use geometry::{baseplate_commands, bin_commands, bin_inside_top, label_areas, label_size, text_depth, text_width, BaseplateOptions, BinOptions, FLOOR_Z, HALF_PITCH, PITCH, UNIT_H};
use serde_json::{Map, Value};
use serde_json::json;

#[cfg(target_arch = "wasm32")]
mod bindings {
    wit_bindgen::generate!({ world: "command-plugin", path: "wit" });
}

/// Largest bin or baseplate side: 12 full cells (504 mm), and never more than 24 cells.
const MAX_SIDE: f64 = 504.0;
const MAX_CELLS: u64 = 24;
/// Custom cell sizes allowed (mm).
const CUSTOM_MIN: f64 = 15.0;
const CUSTOM_MAX: f64 = 100.0;
const MAX_UNITS: u64 = 20;
const MAX_DIVIDERS: u64 = 10;

fn get_u(p: &Map<String, Value>, key: &str, default: u64, min: u64, max: u64) -> Result<u32, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default as u32),
        Some(v) => match v.as_u64() {
            Some(n) if (min..=max).contains(&n) => Ok(n as u32),
            _ => Err(format!("{key} must be a whole number from {min} to {max}")),
        },
    }
}

/// The first of `keys` that is present: the dialog's name, then older/script names.
fn pick<'a>(p: &'a Map<String, Value>, keys: &[&'a str]) -> (&'a str, Option<&'a Value>) {
    for k in keys {
        if let Some(v) = p.get(*k).filter(|v| !v.is_null()) {
            return (k, Some(v));
        }
    }
    (keys[0], None)
}

fn get_b(p: &Map<String, Value>, keys: &[&str], default: bool) -> Result<bool, String> {
    let (key, v) = pick(p, keys);
    match v {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(format!("{key} must be true or false")),
    }
}

fn get_origin(p: &Map<String, Value>) -> Result<[f64; 2], String> {
    // The desktop dialog sends position_x / position_y (it draws no list fields);
    // scripts may also use origin_x / origin_y or origin: [x, y].
    let num = |keys: &[&str]| -> Result<Option<f64>, String> {
        match pick(p, keys) {
            (_, None) => Ok(None),
            (k, Some(v)) => v.as_f64().filter(|n| n.is_finite()).map(Some).ok_or(format!("{k} must be a number (mm)")),
        }
    };
    let (ox, oy) = (num(&["position_x", "origin_x"])?, num(&["position_y", "origin_y"])?);
    if ox.is_some() || oy.is_some() {
        return Ok([ox.unwrap_or(0.0), oy.unwrap_or(0.0)]);
    }
    match p.get("origin") {
        None | Some(Value::Null) => Ok([0.0, 0.0]),
        Some(Value::Array(a)) if a.len() == 2 && a.iter().all(|v| v.as_f64().is_some()) => {
            Ok([a[0].as_f64().unwrap(), a[1].as_f64().unwrap()])
        }
        _ => Err("origin must be [x, y] in mm".into()),
    }
}

fn get_name(p: &Map<String, Value>, default: &str) -> Result<String, String> {
    match pick(p, &["title", "name"]).1 {
        None | Some(Value::Null) => Ok(default.into()),
        Some(Value::String(s)) if !s.trim().is_empty() && s.len() <= 60 => Ok(s.trim().into()),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(default.into()),
        _ => Err("the body name must be a short text".into()),
    }
}

const GRID_ERR: &str = "grid must be \"42 mm (full)\", \"21 mm (half)\" or \"Custom\"";

/// Cell size along X and Y. "42 mm (full)" / "21 mm (half)" / "Custom (sizes below)" as the
/// dialog offers them; scripts may say "full" / "half" / "custom". Custom takes custom_size_x
/// and custom_size_y (mm), which are ignored otherwise.
fn get_pitch(p: &Map<String, Value>) -> Result<[f64; 2], String> {
    let s = match p.get("grid") {
        None | Some(Value::Null) => return Ok([PITCH; 2]),
        Some(Value::String(s)) => s.to_ascii_lowercase(),
        _ => return Err(GRID_ERR.into()),
    };
    if s.contains("half") || s.starts_with("21") {
        return Ok([HALF_PITCH; 2]);
    }
    if s.contains("full") || s.starts_with("42") {
        return Ok([PITCH; 2]);
    }
    if !s.starts_with("custom") {
        return Err(GRID_ERR.into());
    }
    let size = |key: &str| match p.get(key) {
        None | Some(Value::Null) => Ok(PITCH),
        Some(v) => v.as_f64().filter(|n| (CUSTOM_MIN..=CUSTOM_MAX).contains(n))
            .ok_or(format!("{key} must be from {CUSTOM_MIN} to {CUSTOM_MAX} mm")),
    };
    Ok([size("custom_size_x")?, size("custom_size_y")?])
}

/// Most cells that fit in MAX_SIDE at this cell size.
fn max_cells(pitch: f64) -> u64 {
    ((MAX_SIDE / pitch + 1e-9).floor() as u64).clamp(1, MAX_CELLS)
}

/// " half", " 40x45 mm" or "" for the automatic body name.
fn grid_label(pitch: [f64; 2]) -> String {
    if pitch == [PITCH; 2] {
        String::new()
    } else if pitch == [HALF_PITCH; 2] {
        " half".into()
    } else {
        format!(" {}x{} mm", pitch[0], pitch[1])
    }
}

pub fn parse_bin(params: &Value) -> Result<BinOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let pitch = get_pitch(p)?;
    // smaller cells may be more of them for the same overall size
    let nx = get_u(p, "width", 1, 1, max_cells(pitch[0]))?;
    let ny = get_u(p, "depth", 1, 1, max_cells(pitch[1]))?;
    let units = get_u(p, "height", 3, 2, MAX_UNITS)?;
    let half = grid_label(pitch);
    let lip = get_b(p, &["stacking_lip"], true)?;
    let solid = match p.get("inside") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => true,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("hollow") => false,
        _ => return Err("inside must be \"Hollow\" or \"Solid (for custom cutouts)\"".into()),
    };
    // only a solid bin has a fill; 0 fills it to the top of the walls
    let mut fill = get_mm(p, "inside_fill_height", 0.0, 0.0, UNIT_H * MAX_UNITS as f64)?;
    if !solid {
        fill = 0.0;
    }
    // leave at least 1 mm of wall above the fill
    let max_fill = ((bin_inside_top(units, lip) - 1.0) * 100.0).round() / 100.0;
    if fill > 0.0 && !(FLOOR_Z..=max_fill).contains(&fill) {
        return Err(format!("Fill height must be from {FLOOR_Z} to {max_fill} mm for a {units}-unit bin{}, or 0 to fill it to the top.",
            if lip { "" } else { " without stacking lip" }));
    }
    let mut label_text: Vec<String> = match p.get("inside_label_text") {
        None | Some(Value::Null) => vec![],
        Some(Value::String(s)) if s.trim().is_empty() => vec![],
        Some(Value::String(s)) => s.split(';').map(|t| t.trim().to_string()).collect(),
        _ => return Err("Label text must be text: one label per compartment, separated by ;".into()),
    };
    // a ; at the end adds no compartment
    while label_text.last().is_some_and(|t| t.is_empty()) {
        label_text.pop();
    }
    let mut o = BinOptions {
        nx,
        ny,
        pitch,
        units,
        magnets: get_b(p, &["holes_magnet", "magnets"], false)?,
        screws: get_b(p, &["holes_screw", "screws"], false)?,
        div_x: get_u(p, "dividers_x", 0, 0, MAX_DIVIDERS)?,
        div_y: get_u(p, "dividers_y", 0, 0, MAX_DIVIDERS)?,
        lip,
        solid,
        fill,
        scoop: get_mm(p, "inside_scoop", 0.0, 0.0, 40.0)?,
        label: get_b(p, &["inside_label_tab"], false)?,
        text_size: get_mm(p, "inside_label_text_size", 0.0, 0.0, 20.0)?,
        text_height: get_mm(p, "inside_label_text_height", 0.4, 0.1, 3.0)?,
        flush: match p.get("inside_label_text_style") {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("raised") => false,
            Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("flush") => true,
            _ => return Err("Label text style must be \"Raised\" or \"Flush (inlaid)\"".into()),
        },
        label_text,
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity{half} bin {nx}x{ny}x{units}"))?,
    };
    if o.label_text.iter().any(|t| !t.is_empty()) {
        check_label_text(&mut o)?;
    }
    Ok(o)
}

/// Label text is slow in the desktop app: SolidCraft checks every letter as a body of its own,
/// about half a second each, and a compartment with a label tab costs about as much. A run may
/// take this many of the two together (30 seconds); sunk (flush) letters count twice.
const LABEL_BUDGET: usize = 50;

/// Label text puts a label tab on the bin, and every label must fit on its tab.
fn check_label_text(o: &mut BinOptions) -> Result<(), String> {
    if o.solid {
        return Err("Label text needs a hollow bin: solid bins have no label tabs.".into());
    }
    o.label = true;
    let areas = label_areas(o);
    if o.label_text.len() > areas.len() {
        return Err(format!("{} labels, but the bin has {} compartments: give one label per compartment, separated by ;, \
            back row first, left to right.", o.label_text.len(), areas.len()));
    }
    let glyphs: usize = o.label_text.iter().map(|t| t.chars().filter(|c| !c.is_whitespace()).count()).sum();
    // sunk text is cut out of the bin as well: each letter costs about twice as much
    let per = if o.flush { 2 } else { 1 };
    let room = LABEL_BUDGET.saturating_sub(areas.len()) / per;
    if glyphs > room {
        return Err(format!("{glyphs} letters and digits of label text take longer than SolidCraft allows a plugin \
            (30 seconds): with {} compartments, use at most {room}. Shorter labels, or fewer compartments, will do.", areas.len()));
    }
    for (t, &(_, _, w, d)) in o.label_text.iter().zip(&areas) {
        if t.is_empty() {
            continue;
        }
        let h = label_size(o, t, w, d);
        if h < 2.0 {
            return Err(format!("\"{t}\" does not fit on its label tab ({w:.0} mm wide): make it shorter or the bin wider."));
        }
        if text_width(t, h) > w + 0.01 || h * text_depth(t) > d + 0.01 {
            return Err(format!("\"{t}\" at {h} mm does not fit on its label tab ({w:.0} x {d:.0} mm): make the text smaller, \
                or set Label text size to 0 to fit it automatically."));
        }
    }
    Ok(())
}

pub fn parse_baseplate(params: &Value) -> Result<BaseplateOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let pitch = get_pitch(p)?;
    let nx = get_u(p, "width", 3, 1, max_cells(pitch[0]))?;
    let ny = get_u(p, "depth", 3, 1, max_cells(pitch[1]))?;
    let open = match p.get("bottom") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("open") => true,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => false,
        _ => return Err("bottom must be \"Solid\" or \"Open (frame only)\"".into()),
    };
    let magnets = get_b(p, &["holes_magnet", "magnets"], false)?;
    if open && magnets {
        return Err("magnet holes need a solid bottom: choose Bottom \"Solid\" or turn Magnet holes off".into());
    }
    let half = grid_label(pitch);
    let frame = if open { " frame" } else { "" };
    Ok(BaseplateOptions {
        nx,
        ny,
        pitch,
        open,
        magnets,
        filler: [
            get_mm(p, "filler_left", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_right", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_front", 0.0, 0.0, MAX_FILLER)?,
            get_mm(p, "filler_back", 0.0, 0.0, MAX_FILLER)?,
        ],
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity{half} baseplate{frame} {nx}x{ny}"))?,
    })
}

/// Most compartments a bin with scoops or label tabs can have in one run: each divider
/// crosses every row's scoop and tab, and 49 already take 25 s.
const MAX_SCOOP_COMPARTMENTS: u32 = 36;

/// Widest filler on one side of a baseplate (mm).
const MAX_FILLER: f64 = 200.0;

/// Largest drawer side (mm).
const MAX_DRAWER: f64 = 1000.0;

/// SolidCraft gives a plugin run 30 s, and the time goes mostly on joining the cells, about
/// 0.15 s a cell, and more with magnet holes, on a 4-core desktop (10 x 10 cells: 16 s, 6 x 6
/// with magnet holes: 15 s, 8 x 8 with them: over 30 s). These keep a run within the limit.
const MAX_CELLS_RUN: u32 = 100;
const MAX_CELLS_RUN_MAGNETS: u32 = 42;

fn check_time(cells: u32, magnets: bool, what: &str) -> Result<(), String> {
    let max = if magnets { MAX_CELLS_RUN_MAGNETS } else { MAX_CELLS_RUN };
    if cells > max {
        let with = if magnets { " with magnet holes" } else { "" };
        return Err(format!(
            "{cells} cells is too many for one run: SolidCraft gives a plugin 30 seconds, which is about {max} cells{with}. {what}"
        ));
    }
    Ok(())
}

fn get_mm(p: &Map<String, Value>, key: &str, default: f64, min: f64, max: f64) -> Result<f64, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().filter(|n| (min..=max).contains(n)).ok_or(format!("{key} must be from {min} to {max} mm")),
    }
}

fn get_filler(p: &Map<String, Value>, key: &str, low: &str, high: &str) -> Result<Filler, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(Filler::Even),
        Some(Value::String(s)) => {
            let s = s.to_ascii_lowercase();
            if s.starts_with("even") || s.starts_with("both") {
                Ok(Filler::Even)
            } else if s.starts_with(low) {
                Ok(Filler::Low)
            } else if s.starts_with(high) {
                Ok(Filler::High)
            } else {
                Err(format!("{key} must be \"Even (both sides)\", \"{low}\" or \"{high}\""))
            }
        }
        _ => Err(format!("{key} must be a choice from the list")),
    }
}

pub fn parse_drawer(params: &Value) -> Result<DrawerOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let pitch = get_pitch(p)?;
    let drawer = [get_mm(p, "drawer_width", 300.0, 15.0, MAX_DRAWER)?, get_mm(p, "drawer_depth", 300.0, 15.0, MAX_DRAWER)?];
    let bed = [get_mm(p, "printer_x", 256.0, 50.0, MAX_DRAWER)?, get_mm(p, "printer_y", 256.0, 50.0, MAX_DRAWER)?];
    let open = match p.get("bottom") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("open") => true,
        Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => false,
        _ => return Err("bottom must be \"Solid\" or \"Open (frame only)\"".into()),
    };
    let magnets = get_b(p, &["holes_magnet", "magnets"], false)?;
    if open && magnets {
        return Err("magnet holes need a solid bottom: choose Bottom \"Solid\" or turn Magnet holes off".into());
    }
    let piece = get_u(p, "piece", 0, 0, 999)?;
    let half = grid_label(pitch);
    Ok(DrawerOptions {
        drawer,
        bed,
        pitch,
        filler: [get_filler(p, "filler_x", "left", "right")?, get_filler(p, "filler_y", "front", "back")?],
        open,
        magnets,
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity{half} drawer {}x{}", drawer[0], drawer[1]))?,
        piece,
    })
}

/// Turns the plan's commands into the ones SolidCraft runs, using what earlier ones reported:
/// `@pattern` becomes `sketch.pattern.rectangular` over the curves drawn since the sketch began
/// (or the last pattern), `@created` in a body list becomes the bodies the last command made,
/// and body and sketch names become the ones SolidCraft actually gave (a second bin with the
/// same name gets another, and must not change the first).
#[derive(Default)]
pub struct Runner {
    drawn: Vec<Value>,
    created: Vec<Value>,
    bodies: std::collections::HashMap<String, String>,
    sketches: std::collections::HashMap<String, Value>,
}

impl Runner {
    pub fn prepare(&self, c: &Value) -> (String, Value) {
        let name = c["command"].as_str().unwrap_or_default();
        if name == "@pattern" {
            return ("sketch.pattern.rectangular".into(), pattern_args(&c["params"], &self.drawn));
        }
        let mut p = c["params"].clone();
        let body = |v: &str| self.bodies.get(v).cloned().unwrap_or_else(|| v.to_string());
        for key in ["targets", "bodies", "tools"] {
            if let Some(Value::Array(list)) = p.get(key) {
                let mut out = Vec::new();
                for v in list {
                    match v.as_str() {
                        Some("@created") => out.extend(self.created.iter().cloned()),
                        Some("@last") => out.extend(self.created.last().cloned()),
                        Some("@rest") => out.extend(self.created.iter().skip(1).cloned()),
                        Some(s) => out.push(json!(body(s))),
                        None => out.push(v.clone()),
                    }
                }
                p[key] = json!(out);
            }
        }
        for key in ["target", "body"] {
            if let Some(s) = p.get(key).and_then(|v| v.as_str()) {
                p[key] = json!(body(s));
            }
        }
        if let Some(s) = p.get("sketch").and_then(|v| v.as_str()) {
            if let Some(id) = self.sketches.get(s) {
                p["sketch"] = id.clone();
            }
        }
        (name.to_string(), p)
    }

    pub fn record(&mut self, c: &Value, reply: &Value) {
        let name = c["command"].as_str().unwrap_or_default();
        let reply = reply.get("result").filter(|r| r.is_object()).unwrap_or(reply);
        match name {
            "@pattern" => self.drawn.clear(),
            "sketch.create" => {
                self.drawn.clear();
                if let (Some(n), Some(id)) = (c["params"]["name"].as_str(), reply.get("sketch")) {
                    self.sketches.insert(n.to_string(), id.clone());
                }
            }
            "sketch.finish" => {}
            n if n.starts_with("sketch.") => {
                if let Some(Value::Array(ids)) = reply.pointer("/created/curves") {
                    self.drawn.extend(ids.iter().cloned());
                }
            }
            _ => {}
        }
        if let Some(Value::Array(made)) = reply.get("created_bodies") {
            // `@created` / `@last`: the copies the last body pattern made; `@rest`: all but the
            // first body an extrude made in pieces (text: a body for each letter)
            if name == "solid.pattern.rectangular" || (name == "solid.extrude" && made.len() > 1) {
                self.created = made.clone();
            }
            // the body this command asked to make, by the name SolidCraft gave it
            if let (Some(asked), Some(Value::String(got))) = (c["params"]["body_name"].as_str(), made.first()) {
                self.bodies.insert(asked.to_string(), got.clone());
            }
        }
    }
}

/// `sketch.pattern.rectangular`'s arguments for an `@pattern` step over the curves `ids`.
pub fn pattern_args(p: &Value, ids: &[Value]) -> Value {
    let (n1, n2) = (p["count"].as_u64().unwrap_or(1), p["count2"].as_u64().unwrap_or(1));
    if n1 == 1 {
        // only along Y
        json!({"entities": ids, "count": n2, "spacing": p["spacing2"], "dir": [0, 1]})
    } else if n2 == 1 {
        json!({"entities": ids, "count": n1, "spacing": p["spacing"]})
    } else {
        json!({"entities": ids, "count": n1, "spacing": p["spacing"], "count2": n2, "spacing2": p["spacing2"]})
    }
}

/// The command list for an entry, or a message saying what is wrong with the input.
pub fn plan(entry: &str, params: &Value) -> Result<(String, Vec<Value>), String> {
    match entry {
        "bin" => {
            let o = parse_bin(params)?;
            let compartments = (o.div_x + 1) * (o.div_y + 1);
            if !o.solid && (o.scoop > 0.0 || o.label) && compartments > MAX_SCOOP_COMPARTMENTS {
                return Err(format!(
                    "{compartments} compartments with a scoop or label tab take longer than SolidCraft allows a plugin \
                     (30 seconds): use at most {MAX_SCOOP_COMPARTMENTS}, or fewer dividers."
                ));
            }
            // holes make the feet slower to join: screw holes (which reach into the block) most
            let weight = 1.0 + if o.magnets { 0.6 } else { 0.0 } + if o.screws { 2.0 } else { 0.0 };
            let cells = o.nx * o.ny;
            if cells as f64 * weight > MAX_CELLS_RUN as f64 {
                let fit = (MAX_CELLS_RUN as f64 / weight).floor();
                let with = match (o.magnets, o.screws) {
                    (true, true) => " with magnet and screw holes",
                    (true, false) => " with magnet holes",
                    (false, true) => " with screw holes",
                    _ => "",
                };
                return Err(format!("{cells} cells is too many for one run: SolidCraft gives a plugin 30 seconds, \
                    which is about {fit} cells{with}. Make a smaller bin."));
            }
            Ok((o.name.clone(), bin_commands(&o)))
        }
        "baseplate" => {
            let o = parse_baseplate(params)?;
            check_time(o.nx * o.ny, o.magnets && !o.open,
                "Use Gridfinity drawer baseplate, which splits it into pieces and can build them one at a time.")?;
            Ok((o.name.clone(), baseplate_commands(&o)))
        }
        "drawer" => {
            let o = parse_drawer(params)?;
            let (pl, c) = drawer_commands(&o)?;
            let count = pl.axes[0].pieces.len() * pl.axes[1].pieces.len();
            let built = drawer::built_cells(&o, &pl);
            check_time(built, o.magnets && !o.open, &if o.piece == 0 {
                format!("This drawer is {count} pieces: set Piece to 1, 2, ... {count} and run once for each.")
            } else {
                "Use a smaller printer size, so the pieces get smaller.".to_string()
            })?;
            Ok((o.name.clone(), c))
        }
        other => Err(format!("unknown entry `{other}` (use `bin`, `baseplate` or `drawer`)")),
    }
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use super::*;
    use bindings::craft::plugin::commands;
    use bindings::craft::plugin::control;
    use bindings::craft::plugin::types::{FieldFailure, PluginError};
    use bindings::exports::craft::plugin::entry::Guest;

    struct Gridfinity;

    impl Guest for Gridfinity {
        fn execute(entry: String, params: String) -> Result<String, PluginError> {
            let params: Value = serde_json::from_str(if params.trim().is_empty() { "{}" } else { &params })
                .map_err(|e| PluginError::InvalidInput(FieldFailure { field: "params".into(), reason: e.to_string() }))?;
            let (body, cmds) = plan(&entry, &params)
                .map_err(|reason| PluginError::InvalidInput(FieldFailure { field: entry.clone(), reason }))?;
            let total = cmds.len();
            let mut run = Runner::default();
            for (i, c) in cmds.iter().enumerate() {
                // progress and cancel checks are host calls too, and a run may make only 256
                if i % 16 == 0 {
                    if control::cancelled() {
                        return Err(PluginError::Cancelled);
                    }
                    control::progress(i as f32 / total as f32, &format!("Gridfinity: step {} of {total}", i + 1));
                }
                let (name, args) = run.prepare(c);
                // nothing to join (the text came out as one piece)
                if name == "solid.combine" && args["tools"].as_array().is_some_and(|t| t.is_empty()) {
                    continue;
                }
                let reply = commands::run(&name, &args.to_string())?;
                run.record(c, &serde_json::from_str(&reply).unwrap_or(Value::Null));
            }
            Ok(json!({"body": body, "commands": total}).to_string())
        }
    }

    bindings::export!(Gridfinity with_types_in bindings);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_limits() {
        let o = parse_bin(&json!({})).unwrap();
        assert_eq!((o.nx, o.ny, o.units, o.lip), (1, 1, 3, true));
        assert!(parse_bin(&json!({"width": 0})).is_err());
        assert!(parse_bin(&json!({"height": 21})).is_err());
        assert!(parse_bin(&json!({"magnets": "yes"})).is_err());
        assert!(plan("lid", &json!({})).is_err());
        assert_eq!(parse_bin(&json!({"origin_x": 50.0, "origin_y": -10})).unwrap().origin, [50.0, -10.0]);
        assert_eq!(parse_bin(&json!({"origin": [1, 2]})).unwrap().origin, [1.0, 2.0]);
        assert!(parse_bin(&json!({"origin_x": "a"})).is_err());
        // the desktop dialog's names
        let o = parse_bin(&json!({"holes_magnet": true, "holes_screw": true, "position_x": 5, "title": "Tray"})).unwrap();
        assert!(o.magnets && o.screws);
        assert_eq!((o.origin, o.name.as_str()), ([5.0, 0.0], "Tray"));
        assert!(parse_baseplate(&json!({"holes_magnet": true})).unwrap().magnets);
        let h = parse_bin(&json!({"grid": "21 mm (half)", "width": 24})).unwrap();
        assert_eq!((h.pitch, h.nx, h.name.as_str()), ([HALF_PITCH; 2], 24, "Gridfinity half bin 24x1x3"));
        assert_eq!(parse_bin(&json!({"grid": "42 mm (full)"})).unwrap().pitch, [PITCH; 2]);
        assert!(parse_bin(&json!({"grid": "huge"})).is_err());
        assert!(parse_bin(&json!({"width": 13})).is_err());
        assert!(parse_bin(&json!({"inside": "Solid (for custom cutouts)"})).unwrap().solid);
        assert!(!parse_bin(&json!({"inside": "Hollow"})).unwrap().solid);
        assert!(parse_bin(&json!({"inside": "liquid"})).is_err());
        let b = parse_baseplate(&json!({"grid": "21 mm (half)", "bottom": "Open (frame only)", "width": 4})).unwrap();
        assert_eq!((b.pitch, b.open, b.name.as_str()), ([HALF_PITCH; 2], true, "Gridfinity half baseplate frame 4x3"));
        assert!(parse_baseplate(&json!({"bottom": "Open (frame only)", "holes_magnet": true})).is_err());
        let (name, cmds) = plan("baseplate", &json!({"width": 2, "depth": 2, "magnets": true})).unwrap();
        assert_eq!(name, "Gridfinity baseplate 2x2");
        assert!(!cmds.is_empty());
        // custom cell sizes
        let c = parse_bin(&json!({"grid": "Custom (sizes below)", "custom_size_x": 40, "custom_size_y": 45.5, "width": 2})).unwrap();
        assert_eq!((c.pitch, c.name.as_str()), ([40.0, 45.5], "Gridfinity 40x45.5 mm bin 2x1x3"));
        assert_eq!(parse_bin(&json!({"grid": "custom"})).unwrap().pitch, [PITCH; 2]);
        // the custom sizes only count when Custom is chosen
        assert_eq!(parse_bin(&json!({"grid": "42 mm (full)", "custom_size_x": 30})).unwrap().pitch, [PITCH; 2]);
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_x": 14})).is_err());
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_y": 101})).is_err());
        // 504 mm at most: 20 cells of 25 mm, 5 of 100 mm
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_x": 25, "width": 20})).is_ok());
        assert!(parse_bin(&json!({"grid": "custom", "custom_size_x": 25, "width": 21})).is_err());
        assert!(parse_baseplate(&json!({"grid": "custom", "custom_size_y": 100, "depth": 6})).is_err());
        let b = parse_baseplate(&json!({"grid": "custom", "custom_size_x": 50, "custom_size_y": 30})).unwrap();
        assert_eq!(b.name, "Gridfinity 50x30 mm baseplate 3x3");
    }

    #[test]
    fn runs_that_would_take_too_long_say_what_to_do() {
        assert!(plan("baseplate", &json!({"width": 10, "depth": 10})).is_ok());
        let e = plan("baseplate", &json!({"width": 12, "depth": 12})).unwrap_err();
        assert!(e.contains("144 cells") && e.contains("drawer baseplate"));
        assert!(plan("baseplate", &json!({"width": 7, "depth": 7, "holes_magnet": true})).is_err());
        assert!(plan("bin", &json!({"width": 7, "depth": 6, "holes_magnet": true})).is_ok());
        assert!(plan("bin", &json!({"width": 7, "depth": 6, "holes_magnet": true, "holes_screw": true})).unwrap_err().contains("27 cells"));
        // a big drawer: all at once is too much, but it says how to build it piece by piece
        let d = json!({"drawer_width": 600, "drawer_depth": 500, "holes_magnet": true});
        let e = plan("drawer", &d).unwrap_err();
        assert!(e.contains("set Piece to 1, 2"), "{e}");
        let mut one = d.clone();
        one["piece"] = json!(1);
        assert!(plan("drawer", &one).is_ok());
    }

    #[test]
    fn baseplate_filler_keeps_the_corner_at_the_origin() {
        let b = parse_baseplate(&json!({"width": 2, "depth": 1, "filler_left": 10, "filler_back": 7.5})).unwrap();
        assert_eq!(b.filler, [10.0, 0.0, 0.0, 7.5]);
        assert!(parse_baseplate(&json!({"filler_right": -1})).is_err());
        assert!(parse_baseplate(&json!({"filler_front": 201})).is_err());
        let (_, cmds) = plan("baseplate", &json!({"width": 2, "depth": 1, "filler_left": 10, "filler_back": 7.5})).unwrap();
        let boxes: Vec<_> = cmds.iter().filter(|c| c["command"] == "solid.box").map(|c| c["params"]["corner"].clone()).collect();
        assert_eq!(boxes, vec![json!([0.0, 0.0, 0.0]), json!([0.0, 42.0, 0.0])]);
        // no filler: the same commands as before
        let (_, a) = plan("baseplate", &json!({"width": 2})).unwrap();
        let (_, z) = plan("baseplate", &json!({"width": 2, "filler_left": 0})).unwrap();
        assert_eq!(a, z);
    }

    #[test]
    fn label_text() {
        // text adds the tab; one label per compartment, back row first
        let o = parse_bin(&json!({"width": 2, "dividers_x": 1, "inside_label_text": "M3x6; M3x8"})).unwrap();
        assert!(o.label);
        assert_eq!(o.label_text, vec!["M3x6", "M3x8"]);
        let (_, c) = plan("bin", &json!({"width": 2, "dividers_x": 1, "inside_label_text": "M3x6;; "})).unwrap();
        let texts: Vec<_> = c.iter().filter(|v| v["command"] == "sketch.text").collect();
        assert_eq!(texts.len(), 1);
        // left compartment, on the back tab
        let at = texts[0]["params"]["at"].as_array().unwrap();
        assert!(at[0].as_f64().unwrap() < 42.0 && at[1].as_f64().unwrap() > 30.0);
        let last = c.last().unwrap();
        assert_eq!((last["command"].as_str(), last["params"]["tools"][0].as_str()), (Some("solid.combine"), Some("@rest")));
        // flush: sunk into the tab, then cut out of the bin keeping the text
        let (_, c) = plan("bin", &json!({"inside_label_text": "M3", "inside_label_text_style": "Flush (inlaid)"})).unwrap();
        let ext = c.iter().rev().find(|v| v["command"] == "solid.extrude").unwrap();
        assert_eq!(ext["params"]["direction"].as_str(), Some("negative"));
        let last = c.last().unwrap();
        assert_eq!((last["params"]["operation"].as_str(), last["params"]["keep_tools"].as_bool()), (Some("cut"), Some(true)));
        assert!(parse_bin(&json!({"inside_label_text_style": "Sunk"})).is_err());
        let flush = json!({"width": 3, "depth": 2, "dividers_x": 2, "dividers_y": 1, "inside_label_text_style": "Flush (inlaid)",
            "inside_label_text": "M3x6 bolt; M3x8 bolt; M3x10 bolt"});
        assert!(parse_bin(&flush).unwrap_err().contains("with 6 compartments, use at most 22"));
        // no text, no text commands
        let (_, c) = plan("bin", &json!({"inside_label_tab": true})).unwrap();
        assert!(!c.iter().any(|v| v["command"] == "sketch.text"));

        assert!(parse_bin(&json!({"inside_label_text": "a; b"})).unwrap_err().contains("2 labels, but the bin has 1 compartments"));
        assert!(parse_bin(&json!({"inside": "Solid (for custom cutouts)", "inside_label_text": "x"})).unwrap_err().contains("hollow"));
        assert!(parse_bin(&json!({"inside_label_text": "A very long label for one small cell"})).unwrap_err().contains("does not fit"));
        assert!(parse_bin(&json!({"width": 2, "inside_label_text": "M3", "inside_label_text_size": 8})).unwrap_err().contains("at 8 mm"));
        let many = vec!["M3x10"; 17].join(";");
        assert!(parse_bin(&json!({"width": 4, "depth": 4, "dividers_x": 5, "dividers_y": 5, "inside_label_text": many}))
            .unwrap_err().contains("85 letters and digits of label text take longer than SolidCraft allows a plugin (30 seconds): with 36 compartments, use at most 14"));
    }

    #[test]
    fn scoop_and_label_options() {
        let o = parse_bin(&json!({"inside_scoop": 10, "inside_label_tab": true})).unwrap();
        assert_eq!((o.scoop, o.label), (10.0, true));
        let o = parse_bin(&json!({})).unwrap();
        assert_eq!((o.scoop, o.label), (0.0, false));
        assert!(parse_bin(&json!({"inside_scoop": 41})).is_err());
        // off: the same commands as without the fields
        let (_, a) = plan("bin", &json!({"width": 2, "dividers_x": 1})).unwrap();
        let (_, z) = plan("bin", &json!({"width": 2, "dividers_x": 1, "inside_scoop": 0, "inside_label_tab": false})).unwrap();
        assert_eq!(a, z);
        // on: one YZ sketch with a profile per row and per feature, joined, then trimmed
        let (_, c) = plan("bin", &json!({"width": 2, "dividers_y": 2, "inside_scoop": 8, "inside_label_tab": true})).unwrap();
        assert_eq!(c.iter().filter(|v| v["command"] == "sketch.create" && v["params"]["plane"] == "YZ").count(), 1);
        assert_eq!(c.iter().filter(|v| v["command"] == "sketch.line").count(), 6);
        // a solid bin has no inside to put them in
        let (_, s) = plan("bin", &json!({"inside": "Solid (for custom cutouts)", "inside_scoop": 8, "inside_label_tab": true})).unwrap();
        assert!(!s.iter().any(|v| v["params"]["plane"] == "YZ"));
        // too many compartments for one run
        assert!(plan("bin", &json!({"width": 4, "depth": 4, "dividers_x": 6, "dividers_y": 6, "inside_label_tab": true})).unwrap_err().contains("49 compartments"));
        assert!(plan("bin", &json!({"width": 4, "depth": 4, "dividers_x": 6, "dividers_y": 6})).is_ok());
    }

    #[test]
    fn fill_height() {
        let solid = "Solid (for custom cutouts)";
        // off, or on a hollow bin: the same commands as without the field
        let (_, a) = plan("bin", &json!({"inside": solid, "height": 6})).unwrap();
        let (_, z) = plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": 0})).unwrap();
        assert_eq!(a, z);
        let (_, h) = plan("bin", &json!({"height": 6})).unwrap();
        let (_, hf) = plan("bin", &json!({"height": 6, "inside_fill_height": 20})).unwrap();
        assert_eq!(h, hf);
        // on: a cavity cut down to the fill, under the lip support
        let (_, c) = plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": 20})).unwrap();
        let cav = c.iter().find(|v| v["command"] == "solid.extrude" && v["params"]["sketch"].as_str().unwrap().ends_with("Cavity")).unwrap();
        let top = bin_inside_top(6, true);
        assert!((cav["params"]["distance"].as_f64().unwrap() - (top - 20.0)).abs() < 1e-6);
        assert!(c.iter().any(|v| v["params"]["sketch"].as_str().map_or(false, |s| s.ends_with("Support1"))));
        assert!(!c.iter().any(|v| v["command"] == "solid.box"));
        // out of range
        assert!(plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": 5})).unwrap_err().contains("Fill height"));
        assert!(plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": top})).is_err());
        assert!(plan("bin", &json!({"inside": solid, "height": 6, "inside_fill_height": top - 1.0})).is_ok());
    }
}
