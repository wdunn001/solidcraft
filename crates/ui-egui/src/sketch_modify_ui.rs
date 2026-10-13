//! Tool windows for sketch-space modify commands, on the `dialogs_surface` pattern (#19): one
//! `Mod` enum behind one `Kind::SketchModify` variant, grown by one variant and one arm per
//! command rather than by a new module each time. Every model-space modify command already has
//! a tool window; no sketch-space one did.
//!
//! `Mod::Offset` serves `sketch.offset` (upstream issue #162 item 1: "Offset does not create a
//! constraint, the size of the offset seems random").
//!
//! `Mod::Corner` serves `sketch.fillet` and the three `sketch.chamfer.*` commands together, the
//! way `solid.fillet` and `solid.chamfer` already share one window. Before it, a click committed
//! a size guessed at 20 percent of the shorter leg and then opened a bare parameter editor, so
//! the design changed before any number was asked for. `sketch.chamfer.distance_angle` sent a
//! hard-coded 45 degrees and `sketch.chamfer.two_distance` forced its second distance equal to
//! the first, leaving both values unreachable from the UI.
//!
//! The sketch patterns, `sketch.scale`, `sketch.mirror` and `sketch.move` are the same gap and
//! join this module later, each its own change.
//!
//! The Chamfer Type choice is a row of icon buttons rather than a drop-down: the modes differ
//! in shape, and a picture of each shape names it better than a line of text.

use egui::Pos2;
use serde_json::{Map, Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::doc::expr::Kind as ValueKind;

use crate::SolveApp;
use crate::dialogs::{CHAMFER_TYPES, Kind, check, field, icon_choice, row_label};
use crate::selection::{CURVES, POINTS, SelInput};
use crate::viewport::{Hit, Proj};

/// The three chamfer types, drawn as the cut each one makes, in `CHAMFER_TYPES` order.
const CHAMFER_TYPE_ICONS: [&str; 3] = ["sketch_chamfer_equal", "sketch_chamfer_two", "sketch_chamfer_angle"];

/// A sketch modify tool window's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Mod {
    /// `sketch.offset`: the curves to offset, the distance typed and whether the connected
    /// chain follows. A positive distance is left of the chain, as the command defines it;
    /// `flip` reverses whatever sign the field holds.
    Offset { distance: String, chain: bool, flip: bool },
    /// `sketch.fillet` and the three `sketch.chamfer.*` commands. `chamfer` chooses between a
    /// fillet and a chamfer and never changes while the window is open; `ctype` chooses which
    /// of the three chamfer commands runs (0 equal distance, 1 two distances, 2 distance and
    /// angle). `size` is the radius or the first distance, `distance2` the second distance and
    /// `angle` the angle from the first line. `flip` names the two lines in the other order, so
    /// the first distance lands on the other leg of the corner.
    Corner { chamfer: bool, ctype: usize, size: String, distance2: String, angle: String, flip: bool },
}

impl Mod {
    pub fn title(&self) -> &'static str {
        match self {
            Mod::Offset { .. } => "OFFSET",
            Mod::Corner { chamfer: false, .. } => "FILLET",
            Mod::Corner { .. } => "CHAMFER",
        }
    }

    /// The value on the canvas.
    pub fn primary(&mut self) -> Option<(&'static str, ValueKind, &mut String)> {
        Some(match self {
            Mod::Offset { distance, .. } => ("Distance", ValueKind::Length, distance),
            Mod::Corner { chamfer: false, size, .. } => ("Radius", ValueKind::Length, size),
            Mod::Corner { size, .. } => ("Distance", ValueKind::Length, size),
        })
    }
}

fn inputs(k: &Mod) -> Vec<SelInput> {
    match k {
        Mod::Offset { .. } => vec![SelInput::new("Curves", CURVES, true)],
        // A fillet rounds any number of corners to one radius (`points`). Each chamfer command
        // cuts one corner, so its input holds one.
        Mod::Corner { chamfer: false, .. } => vec![SelInput::new("Corners", POINTS, true)],
        Mod::Corner { .. } => vec![SelInput::new("Corner", POINTS, false)],
    }
}

/// The dialog for a sketch modify command.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let corner = |chamfer: bool, ctype: usize| Mod::Corner {
        chamfer,
        ctype,
        size: "2 mm".into(),
        distance2: "2 mm".into(),
        angle: "45 deg".into(),
        flip: false,
    };
    let k = match id {
        "sketch.offset" => Mod::Offset { distance: "2 mm".into(), chain: true, flip: false },
        "sketch.fillet" => corner(false, 0),
        "sketch.chamfer.equal_distance" => corner(true, 0),
        "sketch.chamfer.two_distance" => corner(true, 1),
        "sketch.chamfer.distance_angle" => corner(true, 2),
        _ => return None,
    };
    let i = inputs(&k);
    Some((Kind::SketchModify(k), i))
}

/// The rows after the inputs; true when Enter was pressed in a field.
pub fn rows(ui: &mut egui::Ui, k: &mut Mod) -> bool {
    let mut enter = false;
    match k {
        Mod::Offset { distance, chain, flip } => {
            row_label(ui, "Distance");
            enter |= field(ui, distance);
            ui.end_row();
            row_label(ui, "Flip");
            check(ui, flip);
            ui.end_row();
            row_label(ui, "Chain");
            check(ui, chain);
            ui.end_row();
        }
        Mod::Corner { chamfer, ctype, size, distance2, angle, flip } => {
            if *chamfer {
                row_label(ui, "Chamfer Type");
                icon_choice(ui, &CHAMFER_TYPES, &CHAMFER_TYPE_ICONS, ctype);
                ui.end_row();
            }
            row_label(ui, size_label(*chamfer, *ctype));
            enter |= field(ui, size);
            ui.end_row();
            if *chamfer && *ctype == 1 {
                row_label(ui, "Distance 2");
                enter |= field(ui, distance2);
                ui.end_row();
            }
            if *chamfer && *ctype == 2 {
                row_label(ui, "Angle");
                enter |= field(ui, angle);
                ui.end_row();
            }
            // With two equal distances there is nothing to swap.
            if *chamfer && *ctype > 0 {
                row_label(ui, "Flip");
                check(ui, flip);
                ui.end_row();
            }
        }
    }
    enter
}

/// What the first value means for this corner type.
fn size_label(chamfer: bool, ctype: usize) -> &'static str {
    if !chamfer {
        "Radius"
    } else if ctype == 0 {
        "Distance"
    } else {
        "Distance 1"
    }
}

/// The corner under the cursor while the Fillet or Chamfer window is open: the hover hit and
/// the selection it stands for.
///
/// Corners are resolved through `sketch_tools::corner_point` rather than the ordinary
/// nearest-point test. A rectangle drawn from the origin leaves its own endpoint on top of the
/// fixed origin point, and the nearest-point test returns the origin, which ends no two curves.
/// That resolution chooses among the endpoints that actually make a corner instead.
pub(crate) fn corner_hit(app: &SolveApp, proj: &Proj, pos: Pos2) -> Option<(Hit, Sel)> {
    let Some(Kind::SketchModify(Mod::Corner { chamfer, .. })) = app.dialog.as_ref().map(|d| &d.kind) else { return None };
    let sketch = app.session.active_sketch?;
    let (at, id) = crate::sketch_tools::corner_point(app, proj, pos, !*chamfer)?;
    let sel = Sel::SketchPoint { id: format!("{sketch}:{id}") };
    Some((Hit::SketchPoint { sketch, id, at }, sel))
}

/// Sketch curve ids picked in input `i`.
fn curve_ids(inputs: &[SelInput], i: usize) -> Vec<String> {
    let items = inputs.get(i).map(|x| x.items.as_slice()).unwrap_or(&[]);
    items.iter().filter_map(|x| if let Sel::SketchCurve { id } = x { Some(id.clone()) } else { None }).collect()
}

/// Corner point ids picked in input `i`, within the active sketch. A `POINTS` pick is stored as
/// "<sketch id>:<point id>"; a point selected without the window carries the bare id.
fn corner_ids(app: &SolveApp, inputs: &[SelInput], i: usize) -> Vec<String> {
    let active = app.session.active_sketch;
    let items = inputs.get(i).map(|x| x.items.as_slice()).unwrap_or(&[]);
    items
        .iter()
        .filter_map(|x| match x {
            Sel::SketchPoint { id } => match id.split_once(':') {
                Some((sketch, point)) => (sketch.parse::<u64>().ok() == active).then(|| point.to_string()),
                None => Some(id.clone()),
            },
            _ => None,
        })
        .collect()
}

/// A typed distance as the command wants it: the field evaluated against the document's
/// parameters, with the sign reversed when Flip is set. A bare number means the design's units,
/// as a typed sketch value does everywhere else.
fn signed_length(app: &SolveApp, text: &str, flip: bool) -> Result<f64, String> {
    let doc = &app.session.doc;
    let v = doc.eval(&doc.with_design_unit(text.trim(), ValueKind::Length), ValueKind::Length).map_err(|e| format!("Distance: {e}"))?;
    if !v.is_finite() || v.abs() <= 1e-9 {
        return Err("Distance must be a non-zero number".into());
    }
    Ok(if flip { -v } else { v })
}

/// A typed field evaluated against the document's parameters. A bare number means the design's
/// units for a length and degrees for an angle, as every other typed sketch value does.
fn value(app: &SolveApp, label: &str, text: &str, kind: ValueKind) -> Result<f64, String> {
    let doc = &app.session.doc;
    let v = doc.eval(&doc.with_design_unit(text.trim(), kind), kind).map_err(|e| format!("{label}: {e}"))?;
    if !v.is_finite() || v <= 1e-9 {
        return Err(format!("{label} must be a positive number"));
    }
    Ok(v)
}

/// The corner as a chamfer command takes it. Unflipped it is the point itself, and the command
/// resolves the two lines in curve order. Flipped it names those two lines in the other order,
/// so `distance` applies to the other leg.
fn corner_ref(app: &SolveApp, point: &str, flip: bool) -> Result<Value, String> {
    if !flip {
        return Ok(json!({ "point": point }));
    }
    let gone = || "that corner is no longer in the sketch".to_string();
    let sketch = app.session.active_sketch.ok_or("open a sketch first")?;
    let state = app.session.world_state();
    let sk = &state.sketch(sketch).ok_or_else(gone)?.sketch;
    let index = sk.resolve_point(point).ok_or_else(gone)?;
    let (a, b) = crate::sketch_tools::corner_curves(sk, index, false).ok_or("a chamfer needs a corner of exactly two lines")?;
    let id = |c: usize| sk.curves.get(c).map(|x| x.id.clone()).ok_or_else(gone);
    Ok(json!({"a": id(b)?, "b": id(a)?}))
}

/// The command a sketch modify dialog makes.
///
/// `sketch.offset` takes a plain evaluated number for its distance, not an expression string: it
/// has no model parameter to carry one, unlike `sketch.fillet`'s radius.
///
/// `sketch.fillet` takes its radius as an expression: it makes a model parameter to carry one,
/// and a radius dimension on the arc that follows it. The chamfer commands have no parameter to
/// hold an expression, so their fields are evaluated here into plain numbers.
pub fn commands(app: &SolveApp, k: &Mod, inputs: &[SelInput], extra: &Map<String, Value>) -> Result<Vec<(String, Value)>, String> {
    let (cmd, mut params) = match k {
        Mod::Offset { distance, chain, flip } => {
            let curves = curve_ids(inputs, 0);
            if curves.is_empty() {
                return Err("Select the curves to offset first".into());
            }
            let d = signed_length(app, distance, *flip)?;
            ("sketch.offset", json!({"curves": curves, "distance": d, "chain": chain}))
        }
        Mod::Corner { chamfer: false, size, .. } => {
            let corners = corner_ids(app, inputs, 0);
            if corners.is_empty() {
                return Err("select the corners to round first".into());
            }
            // The radius is checked where it is evaluated, against the document's parameters.
            ("sketch.fillet", json!({"points": corners, "radius": size.trim()}))
        }
        Mod::Corner { ctype, size, distance2, angle, flip, .. } => {
            let point = corner_ids(app, inputs, 0).into_iter().next().ok_or("select the corner to cut first")?;
            let mut p = corner_ref(app, &point, *flip && *ctype > 0)?;
            p["distance"] = json!(value(app, size_label(true, *ctype), size, ValueKind::Length)?);
            match ctype {
                1 => {
                    p["distance2"] = json!(value(app, "Distance 2", distance2, ValueKind::Length)?);
                    ("sketch.chamfer.two_distance", p)
                }
                2 => {
                    // The command takes degrees; an evaluated angle is in radians.
                    p["angle"] = json!(value(app, "Angle", angle, ValueKind::Angle)?.to_degrees());
                    ("sketch.chamfer.distance_angle", p)
                }
                _ => ("sketch.chamfer.equal_distance", p),
            }
        }
    };
    if let Value::Object(m) = &mut params {
        for (key, value) in extra {
            m.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(vec![(cmd.to_string(), params)])
}
