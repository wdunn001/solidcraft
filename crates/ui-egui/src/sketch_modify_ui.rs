//! Tool windows for sketch-space modify commands, on the `dialogs_surface` pattern (#19): one
//! `Mod` enum behind one `Kind::SketchModify` variant, grown by one variant and one arm per
//! command rather than by a new module each time. The first variant is Offset (upstream issue
//! #162 item 1: "Offset does not create a constraint, the size of the offset seems random").
//! `sketch.fillet`, the three `sketch.chamfer.*` commands, the sketch patterns, `sketch.scale`,
//! `sketch.mirror` and `sketch.move` are the same gap and join this module later, each its own
//! change.

use serde_json::{Map, Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::doc::expr::Kind as ValueKind;

use crate::SolveApp;
use crate::dialogs::{Kind, check, field, row_label};
use crate::selection::{CURVES, SelInput};

/// A sketch modify tool window's values.
#[derive(Clone, Debug, PartialEq)]
pub enum Mod {
    /// `sketch.offset`: the curves to offset, the distance typed and whether the connected
    /// chain follows. A positive distance is left of the chain, as the command defines it;
    /// `flip` reverses whatever sign the field holds.
    Offset { distance: String, chain: bool, flip: bool },
}

impl Mod {
    pub fn title(&self) -> &'static str {
        match self {
            Mod::Offset { .. } => "OFFSET",
        }
    }

    /// The value on the canvas.
    pub fn primary(&mut self) -> Option<(&'static str, ValueKind, &mut String)> {
        Some(match self {
            Mod::Offset { distance, .. } => ("Distance", ValueKind::Length, distance),
        })
    }
}

fn inputs(k: &Mod) -> Vec<SelInput> {
    match k {
        Mod::Offset { .. } => vec![SelInput::new("Curves", CURVES, true)],
    }
}

/// The dialog for a sketch modify command.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    let k = match id {
        "sketch.offset" => Mod::Offset { distance: "2 mm".into(), chain: true, flip: false },
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
    }
    enter
}

/// Sketch curve ids picked in input `i`.
fn curve_ids(inputs: &[SelInput], i: usize) -> Vec<String> {
    let items = inputs.get(i).map(|x| x.items.as_slice()).unwrap_or(&[]);
    items.iter().filter_map(|x| if let Sel::SketchCurve { id } = x { Some(id.clone()) } else { None }).collect()
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

/// The command a sketch modify dialog makes. `sketch.offset` takes a plain evaluated number for
/// its distance, not an expression string: it has no model parameter to carry one, unlike
/// `sketch.fillet`'s radius.
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
    };
    if let Value::Object(m) = &mut params {
        for (key, value) in extra {
            m.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(vec![(cmd.to_string(), params)])
}
