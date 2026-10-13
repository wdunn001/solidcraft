//! Tool windows for sketch-space modify commands, on the `dialogs_surface` pattern (#19): one
//! `Mod` enum behind one `Kind::SketchModify` variant, grown by one variant and one arm per
//! command rather than by a new module per command. Every model-space modify command already
//! has a tool window; no sketch-space one did.
//!
//! The first variant here is `sketch.pattern.rectangular`. Its click tool sent a hard-coded
//! count of 3 and had no way to reach the command's second count and spacing, so a user who
//! wanted five instances got three and deleted two. `sketch.offset`, `sketch.fillet`, the three
//! `sketch.chamfer.*` commands, `sketch.pattern.circular`, `sketch.scale`, `sketch.mirror` and
//! `sketch.move` are the same gap and join this module, each as its own change.

use serde_json::{Map, Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::doc::expr::Kind as ValueKind;
use solidcraft_engine::geom::{Vec2, Vec3};

use crate::SolveApp;
use crate::dialogs::{Dialog, Kind, field, row_label};
use crate::selection::{AXES, CURVES, POINTS, SelInput};

/// The engine's own cap on the copies one sketch pattern may make.
const MAX_COPIES: f64 = 500.0;

/// A sketch modify tool window's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Mod {
    /// `sketch.pattern.rectangular`: how many instances along the picked direction and how far
    /// apart, then the same along the perpendicular direction the command derives from it.
    PatternRect { count: String, spacing: String, count2: String, spacing2: String },
}

impl Mod {
    pub fn title(&self) -> &'static str {
        match self {
            Mod::PatternRect { .. } => "RECTANGULAR PATTERN",
        }
    }

    /// The value on the canvas.
    pub fn primary(&mut self) -> Option<(&'static str, ValueKind, &mut String)> {
        Some(match self {
            Mod::PatternRect { spacing, .. } => ("Spacing", ValueKind::Length, spacing),
        })
    }
}

fn inputs(k: &Mod) -> Vec<SelInput> {
    match k {
        // The direction is optional: `sketch.pattern.rectangular` documents sketch x as its
        // default, the way the engine reads it.
        Mod::PatternRect { .. } => vec![SelInput::new("Entities", CURVES | POINTS, true), SelInput::new("Direction", AXES, false)],
    }
}

/// The dialog for a sketch modify command.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let k = match id {
        "sketch.pattern.rectangular" => Mod::PatternRect { count: "3".into(), spacing: "20 mm".into(), count2: "1".into(), spacing2: "20 mm".into() },
        _ => return None,
    };
    let i = inputs(&k);
    Some((Kind::SketchModify(k), i))
}

/// The rows after the inputs; true when Enter was pressed in a field.
pub fn rows(ui: &mut egui::Ui, k: &mut Mod) -> bool {
    let mut enter = false;
    match k {
        Mod::PatternRect { count, spacing, count2, spacing2 } => {
            for (label, value) in [("Quantity", count), ("Spacing", spacing), ("Quantity 2", count2), ("Spacing 2", spacing2)] {
                row_label(ui, label);
                enter |= field(ui, value);
                ui.end_row();
            }
        }
    }
    enter
}

/// Sketch entity ids picked in input `i`. A curve id goes through as it is; a point pick
/// carries `<sketch>:<point>` and the command's `entities` resolves the bare point id.
fn entity_ids(inputs: &[SelInput], i: usize) -> Vec<String> {
    let items = inputs.get(i).map(|x| x.items.as_slice()).unwrap_or(&[]);
    items
        .iter()
        .filter_map(|x| match x {
            Sel::SketchCurve { id } => Some(id.clone()),
            Sel::SketchPoint { id } => Some(id.split_once(':').map(|(_, p)| p.to_string()).unwrap_or_else(|| id.clone())),
            _ => None,
        })
        .collect()
}

/// The in-plane direction a picked origin axis or straight sketch curve gives the active
/// sketch, as `sketch.pattern.rectangular` reads its `dir`. None when there is no active
/// sketch, or when the axis stands perpendicular to the sketch plane and so has no direction
/// in it.
fn sketch_dir(app: &SolveApp, sel: &Sel) -> Option<Vec2> {
    let dir = crate::dialogs::axis_of(app, sel)?.1;
    let plane = app.session.world_state().sketch(app.session.active_sketch?)?.plane;
    Vec2::new(dir.dot(plane.x), dir.dot(plane.y)).normalized()
}

/// The world direction a sketch modify window's dragged value grows along, for the viewport's
/// value box and its drag arrow (the same machinery `dialogs_sheet::arrow` feeds). The
/// rectangular pattern's spacing follows the picked direction, or the sketch's own x axis, which
/// is what the command uses when nothing is picked.
pub fn arrow(app: &SolveApp, d: &Dialog) -> Option<Vec3> {
    let Kind::SketchModify(k) = &d.kind else { return None };
    let plane = app.session.world_state().sketch(app.session.active_sketch?)?.plane;
    match k {
        Mod::PatternRect { .. } => {
            let dir = match d.inputs.get(1).and_then(|i| i.items.first()) {
                Some(sel) => sketch_dir(app, sel)?,
                None => Vec2::X,
            };
            Some(plane.dir_to_world(dir))
        }
    }
}

/// A typed count as the command wants it: the field evaluated against the document's
/// parameters, then a whole number the engine accepts.
///
/// The sketch patterns read `count` and `angle` with `num`, which takes a JSON number and
/// nothing else. An expression string would be refused outright for `count`, and silently
/// replaced by the 360 degree default for `angle`. That silent substitution is the defect
/// upstream #218 reports for `solid.pattern.circular`'s axis origin. The window therefore
/// evaluates every value here and sends numbers.
fn whole(app: &SolveApp, label: &str, text: &str) -> Result<u64, String> {
    let v = app.session.doc.eval(text.trim(), ValueKind::Unitless).map_err(|e| format!("{label}: {e}"))?;
    if !v.is_finite() || !(1.0..=MAX_COPIES).contains(&v) || (v - v.round()).abs() > 1e-9 {
        return Err(format!("{label} must be a whole number from 1 to {MAX_COPIES:.0}"));
    }
    Ok(v.round() as u64)
}

/// A typed length as the command wants it, in millimetres. A bare number means the design's
/// units, as a typed sketch value does everywhere else.
fn length(app: &SolveApp, label: &str, text: &str) -> Result<f64, String> {
    let doc = &app.session.doc;
    let v = doc.eval(&doc.with_design_unit(text.trim(), ValueKind::Length), ValueKind::Length).map_err(|e| format!("{label}: {e}"))?;
    if !v.is_finite() {
        return Err(format!("{label} must be a number"));
    }
    Ok(v)
}

/// The command a sketch modify dialog makes.
pub fn commands(app: &SolveApp, k: &Mod, inputs: &[SelInput], extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    let (cmd, mut params) = match k {
        Mod::PatternRect { count, spacing, count2, spacing2 } => {
            let entities = entity_ids(inputs, 0);
            if entities.is_empty() {
                return Err("Select the sketch curves or points to pattern first".into());
            }
            let n1 = whole(app, "Quantity", count)?;
            let s1 = length(app, "Spacing", spacing)?;
            let n2 = whole(app, "Quantity 2", count2)?;
            let s2 = length(app, "Spacing 2", spacing2)?;
            if n1 > 1 && s1.abs() <= 1e-9 {
                return Err("Spacing must be a non-zero number".into());
            }
            if n2 > 1 && s2.abs() <= 1e-9 {
                return Err("Spacing 2 must be a non-zero number".into());
            }
            if n1.saturating_mul(n2) > MAX_COPIES as u64 {
                return Err(format!("Quantity and Quantity 2 together must make at most {MAX_COPIES:.0} instances"));
            }
            let mut p = json!({"entities": entities, "count": n1, "spacing": s1, "count2": n2, "spacing2": s2});
            // A picked direction that has no component in the sketch plane is reported rather
            // than quietly replaced by the default.
            if let Some(sel) = inputs.get(1).and_then(|i| i.items.first()) {
                let d = sketch_dir(app, sel).ok_or("The direction must be an axis or a straight sketch curve that lies in the sketch plane")?;
                p["dir"] = json!([d.x, d.y]);
            }
            ("sketch.pattern.rectangular", p)
        }
    };
    if let Value::Object(m) = &mut params {
        for (key, value) in extra {
            m.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(vec![(cmd.to_string(), params)])
}
