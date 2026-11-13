//! Gridfinity plugin for SolidCraft: entries `bin` and `baseplate` build
//! ordinary, editable SolidCraft features through the host's command API.

pub mod geometry;

use geometry::{baseplate_commands, bin_commands, BaseplateOptions, BinOptions};
use serde_json::{json, Map, Value};

#[cfg(target_arch = "wasm32")]
mod bindings {
    wit_bindgen::generate!({ world: "command-plugin", path: "wit" });
}

const MAX_CELLS: u64 = 12;
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

pub fn parse_bin(params: &Value) -> Result<BinOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let nx = get_u(p, "width", 1, 1, MAX_CELLS)?;
    let ny = get_u(p, "depth", 1, 1, MAX_CELLS)?;
    let units = get_u(p, "height", 3, 2, MAX_UNITS)?;
    Ok(BinOptions {
        nx,
        ny,
        units,
        magnets: get_b(p, &["holes_magnet", "magnets"], false)?,
        screws: get_b(p, &["holes_screw", "screws"], false)?,
        div_x: get_u(p, "dividers_x", 0, 0, MAX_DIVIDERS)?,
        div_y: get_u(p, "dividers_y", 0, 0, MAX_DIVIDERS)?,
        lip: get_b(p, &["stacking_lip"], true)?,
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity bin {nx}x{ny}x{units}"))?,
    })
}

pub fn parse_baseplate(params: &Value) -> Result<BaseplateOptions, String> {
    let empty = Map::new();
    let p = params.as_object().unwrap_or(&empty);
    let nx = get_u(p, "width", 3, 1, MAX_CELLS)?;
    let ny = get_u(p, "depth", 3, 1, MAX_CELLS)?;
    Ok(BaseplateOptions {
        nx,
        ny,
        magnets: get_b(p, &["holes_magnet", "magnets"], false)?,
        origin: get_origin(p)?,
        name: get_name(p, &format!("Gridfinity baseplate {nx}x{ny}"))?,
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
        let (name, cmds) = plan("baseplate", &json!({"width": 2, "depth": 2, "magnets": true})).unwrap();
        assert_eq!(name, "Gridfinity baseplate 2x2");
        assert!(!cmds.is_empty());
    }
}
