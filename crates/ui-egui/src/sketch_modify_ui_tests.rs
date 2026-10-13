use serde_json::json;
use solidcraft_engine::Sel;

use crate::dialogs::{Kind, apply_commands};
use crate::sketch_modify_ui::Mod;

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
