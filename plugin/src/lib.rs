//! Gridfinity plugin for SolidCraft: entries `bin` and `baseplate` build
//! ordinary, editable SolidCraft features through the host's command API.

pub mod geometry;

use geometry::{baseplate_commands, bin_commands, BaseplateOptions, BinOptions, HALF_PITCH, PITCH};
use serde_json::{Map, Value};
#[cfg(any(test, target_arch = "wasm32"))]
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
    Ok(BinOptions {
        nx,
        ny,
        pitch,
        units,
        magnets: get_b(p, &["holes_magnet", "magnets"], false)?,
        screws: get_b(p, &["holes_screw", "screws"], false)?,
        div_x: get_u(p, "dividers_x", 0, 0, MAX_DIVIDERS)?,
        div_y: get_u(p, "dividers_y", 0, 0, MAX_DIVIDERS)?,
        lip: get_b(p, &["stacking_lip"], true)?,
        solid: match p.get("inside") {
            None | Some(Value::Null) => false,
            Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("solid") => true,
            Some(Value::String(s)) if s.to_ascii_lowercase().starts_with("hollow") => false,
            _ => return Err("inside must be \"Hollow\" or \"Solid (for custom cutouts)\"".into()),
        },
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity{half} bin {nx}x{ny}x{units}"))?,
    })
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
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity{half} baseplate{frame} {nx}x{ny}"))?,
    })
}

/// The command list for an entry, or a message saying what is wrong with the input.
pub fn plan(entry: &str, params: &Value) -> Result<(String, Vec<Value>), String> {
    match entry {
        "bin" => parse_bin(params).map(|o| (o.name.clone(), bin_commands(&o))),
        "baseplate" => parse_baseplate(params).map(|o| (o.name.clone(), baseplate_commands(&o))),
        other => Err(format!("unknown entry `{other}` (use `bin` or `baseplate`)")),
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
            for (i, c) in cmds.iter().enumerate() {
                if control::cancelled() {
                    return Err(PluginError::Cancelled);
                }
                control::progress(i as f32 / total as f32, &format!("Gridfinity: step {} of {total}", i + 1));
                let name = c["command"].as_str().unwrap_or_default();
                commands::run(name, &c["params"].to_string())?;
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
}
