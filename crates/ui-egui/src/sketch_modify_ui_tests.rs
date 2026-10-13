//! Tests for the sketch modify tool windows.
//!
//! The rectangular pattern's click tool could only ever send a count of 3, and the Sketch Scale
//! button reached no tool and no dialog at all, so a press ran `sketch.scale` with no parameters
//! and the engine refused it.

use serde_json::json;
use solidcraft_engine::Sel;

use crate::dialogs::{Kind, apply_commands};
use crate::scenario::Harness;
use crate::selection::SelInput;
use crate::sketch_modify_ui::{Mod, commands};

/// A sketch holding one circle, with the window for `cmd` open on it.
fn open(cmd: &str) -> (crate::scenario::Harness, String) {
    let mut h = crate::scenario::Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.run("sketch.circle.center", json!({"center": [0, 0], "radius": 5})).unwrap();
    h.settle();
    let sid = h.app.session.active_sketch.unwrap();
    let id = h.app.session.model.state().sketch(sid).unwrap().sketch.curves[0].id.clone();
    h.step(&json!({"start": cmd})).unwrap();
    (h, id)
}

fn rect(h: &mut crate::scenario::Harness, count: &str, spacing: &str, count2: &str, spacing2: &str) {
    let d = h.app.dialog.as_mut().expect("the window opened");
    if let Kind::SketchModify(Mod::PatternRect { count: c1, spacing: s1, count2: c2, spacing2: s2 }) = &mut d.kind {
        *c1 = count.into();
        *s1 = spacing.into();
        *c2 = count2.into();
        *s2 = spacing2.into();
    } else {
        panic!("not the rectangular pattern window: {:?}", d.kind);
    }
}

#[test]
fn rectangular_pattern_window_opens_instead_of_a_click_tool() {
    let (h, _) = open("sketch.pattern.rectangular");
    assert!(h.app.tool.is_none(), "the click tool still takes the command");
    let d = h.app.dialog.as_ref().expect("the window opened");
    assert!(matches!(d.kind, Kind::SketchModify(Mod::PatternRect { .. })), "{:?}", d.kind);
    assert_eq!(d.inputs.iter().map(|i| i.label).collect::<Vec<_>>(), vec!["Entities", "Direction"]);
    // Nothing is committed by opening the window.
    let sid = h.app.session.active_sketch.unwrap();
    assert_eq!(h.app.session.model.state().sketch(sid).unwrap().sketch.curves.len(), 1);
}

#[test]
fn typed_counts_and_spacings_reach_the_command_with_the_second_direction() {
    let (mut h, id) = open("sketch.pattern.rectangular");
    h.app.dialog.as_mut().unwrap().inputs[0].items = vec![Sel::SketchCurve { id }];
    rect(&mut h, "5", "20", "2", "15 mm");
    let d = h.app.dialog.clone().unwrap();
    let cmds = apply_commands(&h.app, &d).unwrap();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0].0, "sketch.pattern.rectangular");
    // The click tool could only ever send a count of 3 and no second count at all.
    assert_eq!(cmds[0].1["count"], json!(5));
    assert_eq!(cmds[0].1["spacing"], json!(20.0));
    assert_eq!(cmds[0].1["count2"], json!(2));
    assert_eq!(cmds[0].1["spacing2"], json!(15.0));
    assert!(cmds[0].1.get("dir").is_none(), "an unpicked direction leaves the command's own default");
}

#[test]
fn a_picked_axis_becomes_an_in_plane_direction() {
    let (mut h, id) = open("sketch.pattern.rectangular");
    h.app.dialog.as_mut().unwrap().inputs[0].items = vec![Sel::SketchCurve { id }];
    h.app.dialog.as_mut().unwrap().inputs[1].items = vec![Sel::Axis { name: "Y".into() }];
    rect(&mut h, "3", "10", "1", "10");
    let d = h.app.dialog.clone().unwrap();
    let cmds = apply_commands(&h.app, &d).unwrap();
    assert_eq!(cmds[0].1["dir"], json!([0.0, 1.0]), "sketch y on the XY plane");
}

#[test]
fn an_axis_perpendicular_to_the_sketch_is_reported_not_silently_replaced() {
    let (mut h, id) = open("sketch.pattern.rectangular");
    h.app.dialog.as_mut().unwrap().inputs[0].items = vec![Sel::SketchCurve { id }];
    h.app.dialog.as_mut().unwrap().inputs[1].items = vec![Sel::Axis { name: "Z".into() }];
    rect(&mut h, "3", "10", "1", "10");
    let d = h.app.dialog.clone().unwrap();
    let e = apply_commands(&h.app, &d).unwrap_err();
    assert!(e.contains("sketch plane"), "{e}");
}

/// Upstream #218 is the same shape: `solid.pattern.circular` took expressions in its axis origin
/// and stored `[0, 0, 0]`. The sketch patterns read `count` and `angle` with `num`, which takes a
/// JSON number only, so the window evaluates every field itself.
#[test]
fn a_count_expression_is_evaluated_rather_than_sent_as_a_string() {
    let (mut h, id) = open("sketch.pattern.rectangular");
    h.app.run("parameters.add", json!({"name": "n", "expression": "4", "unit": ""})).unwrap();
    h.app.dialog.as_mut().unwrap().inputs[0].items = vec![Sel::SketchCurve { id }];
    rect(&mut h, "n", "10", "1", "10");
    let d = h.app.dialog.clone().unwrap();
    let cmds = apply_commands(&h.app, &d).unwrap();
    assert_eq!(cmds[0].1["count"], json!(4));
}

#[test]
fn bad_counts_and_an_empty_selection_are_refused_before_the_command_runs() {
    let (mut h, id) = open("sketch.pattern.rectangular");
    rect(&mut h, "5", "20", "1", "20");
    let d = h.app.dialog.clone().unwrap();
    assert!(apply_commands(&h.app, &d).unwrap_err().contains("Select"));
    h.app.dialog.as_mut().unwrap().inputs[0].items = vec![Sel::SketchCurve { id }];
    for bad in ["0", "2.5", "501", "wat"] {
        rect(&mut h, bad, "20", "1", "20");
        let d = h.app.dialog.clone().unwrap();
        assert!(apply_commands(&h.app, &d).is_err(), "count {bad} was accepted");
    }
    rect(&mut h, "5", "0", "1", "20");
    let d = h.app.dialog.clone().unwrap();
    assert!(apply_commands(&h.app, &d).unwrap_err().contains("Spacing"));
}

/// The spacing is a geometric value, so it gets the viewport's value box and a drag arrow
/// through the existing `canvas::anchor` machinery: along the sketch's own x by default, along
/// the picked direction once one is picked.
#[test]
fn the_spacing_gets_a_canvas_arrow_along_the_pattern_direction() {
    let (mut h, id) = open("sketch.pattern.rectangular");
    h.app.dialog.as_mut().unwrap().inputs[0].items = vec![Sel::SketchCurve { id }];
    let d = h.app.dialog.clone().unwrap();
    let (_, dir) = crate::canvas::anchor(&h.app, &d).expect("an anchor on the picked curve");
    assert_eq!(dir, Some(solidcraft_engine::geom::Vec3::X));
    h.app.dialog.as_mut().unwrap().inputs[1].items = vec![Sel::Axis { name: "Y".into() }];
    let d = h.app.dialog.clone().unwrap();
    let (_, dir) = crate::canvas::anchor(&h.app, &d).expect("an anchor on the picked curve");
    assert_eq!(dir, Some(solidcraft_engine::geom::Vec3::Y));
}

/// A sketch on XY holding one 20 by 10 rectangle at the origin.
fn rectangle() -> Harness {
    let mut h = Harness::new();
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [20, 10]})).unwrap();
    h.settle();
    h
}

fn curve_ids(h: &Harness) -> Vec<String> {
    let sid = h.app.session.active_sketch.unwrap();
    let st = h.app.session.model.state();
    st.sketch(sid).unwrap().sketch.curves.iter().map(|c| c.id.clone()).collect()
}

#[test]
fn the_toolbar_button_opens_a_window_instead_of_reporting_an_engine_error() {
    let mut h = rectangle();
    let before = curve_ids(&h).len();
    h.step(&json!({"start": "sketch.scale"})).unwrap();
    let d = h.app.dialog.as_ref().expect("sketch.scale opens a tool window");
    assert!(matches!(d.kind, Kind::SketchModify(Mod::Scale { .. })), "{:?}", d.kind);
    assert_eq!(d.inputs.iter().map(|i| i.label).collect::<Vec<_>>(), vec!["Entities", "Base point"]);
    // Opening the window commits nothing, and the old dead button's error is gone.
    assert_eq!(curve_ids(&h).len(), before);
    assert!(!h.app.status.as_ref().is_some_and(|(_, _, error)| *error), "{:?}", h.app.status);
}

#[test]
fn the_typed_factor_reaches_the_command_unchanged() {
    let h = rectangle();
    let ids = curve_ids(&h);
    let mut inputs = vec![SelInput::new("Entities", crate::selection::CURVES, true), SelInput::new("Base point", crate::selection::POINTS, false)];
    inputs[0].items = ids.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect();
    let k = Mod::Scale { factor: "2.5".into() };
    let out = commands(&h.app, &k, &inputs, &serde_json::Map::new()).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "sketch.scale");
    assert_eq!(out[0].1["factor"], json!(2.5));
    assert_eq!(out[0].1["entities"], json!(ids));
    // No base point picked: the command's own default, the sketch origin, stands.
    assert!(out[0].1.get("base").is_none(), "{}", out[0].1);
}

#[test]
fn an_expression_in_the_factor_field_is_evaluated() {
    let mut h = rectangle();
    h.app.run("parameters.add", json!({"name": "growth", "expression": "3", "unit": ""})).unwrap();
    let ids = curve_ids(&h);
    let mut inputs = vec![SelInput::new("Entities", crate::selection::CURVES, true)];
    inputs[0].items = ids.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect();
    let out = commands(&h.app, &Mod::Scale { factor: "growth * 2".into() }, &inputs, &serde_json::Map::new()).unwrap();
    assert_eq!(out[0].1["factor"], json!(6.0));
}

#[test]
fn a_picked_base_point_loses_the_sketch_prefix_that_picking_adds() {
    let h = rectangle();
    let sid = h.app.session.active_sketch.unwrap();
    let ids = curve_ids(&h);
    let mut inputs = vec![SelInput::new("Entities", crate::selection::CURVES, true), SelInput::new("Base point", crate::selection::POINTS, false)];
    inputs[0].items = vec![Sel::SketchCurve { id: ids[0].clone() }];
    let point = {
        let st = h.app.session.model.state();
        st.sketch(sid).unwrap().sketch.points.first().unwrap().id.clone()
    };
    inputs[1].items = vec![Sel::SketchPoint { id: format!("{sid}:{point}") }];
    let out = commands(&h.app, &Mod::Scale { factor: "2".into() }, &inputs, &serde_json::Map::new()).unwrap();
    assert_eq!(out[0].1["base"], json!(point));
}

#[test]
fn an_empty_selection_or_a_bad_factor_is_an_error_the_window_shows() {
    let h = rectangle();
    let ids = curve_ids(&h);
    let empty = vec![SelInput::new("Entities", crate::selection::CURVES, true)];
    assert!(commands(&h.app, &Mod::Scale { factor: "2".into() }, &empty, &serde_json::Map::new()).is_err());
    let mut filled = empty.clone();
    filled[0].items = vec![Sel::SketchCurve { id: ids[0].clone() }];
    for bad in ["0", "-2", "", "nonsense"] {
        let r = commands(&h.app, &Mod::Scale { factor: bad.into() }, &filled, &serde_json::Map::new());
        assert!(r.is_err(), "factor `{bad}` should not reach the command: {r:?}");
    }
}

#[test]
fn the_window_scales_the_picked_geometry_and_leaves_the_rest_alone() {
    let mut h = rectangle();
    h.app.run("sketch.circle.center", json!({"center": [60, 0], "radius": 5})).unwrap();
    h.settle();
    let sid = h.app.session.active_sketch.unwrap();
    let ids = curve_ids(&h);
    let rect: Vec<String> = ids.iter().take(4).cloned().collect();
    h.step(&json!({"start": "sketch.scale"})).unwrap();
    {
        let d = h.app.dialog.as_mut().unwrap();
        d.inputs[0].items = rect.iter().map(|id| Sel::SketchCurve { id: id.clone() }).collect();
        d.kind = Kind::SketchModify(Mod::Scale { factor: "2".into() });
    }
    // A frame with the picks in place: the canvas value box takes focus on the frame after a
    // pick, and Enter only applies once it has had it.
    h.frames(2);
    h.step(&json!({"key": "Enter"})).unwrap();
    h.settle();
    assert!(h.app.dialog.is_none(), "Enter applies and closes the window");
    let st = h.app.session.model.state();
    let sk = &st.sketch(sid).unwrap().sketch;
    let xs: Vec<f64> = sk.points.iter().map(|p| p.pos.x).collect();
    let ys: Vec<f64> = sk.points.iter().map(|p| p.pos.y).collect();
    // The rectangle doubled about the sketch origin; the circle at x = 60 did not move.
    assert!(xs.iter().any(|x| (x - 40.0).abs() < 1e-6), "{xs:?}");
    assert!(ys.iter().any(|y| (y - 20.0).abs() < 1e-6), "{ys:?}");
    assert!(xs.iter().any(|x| (x - 60.0).abs() < 1e-6), "{xs:?}");
    assert_eq!(sk.curves.len(), 5);
}
