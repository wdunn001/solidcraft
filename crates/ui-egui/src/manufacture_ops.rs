//! Persist Manufacture dialog drafts through `cam.op.replace`. No `crates/manufacture`.

use serde_json::json;
use solidcraft_engine::doc::cam::{CamOperation, SetupOperationType};

use crate::SolveApp;
use crate::mfg_draft::{self, MfgSetup, Operation, Tool};

pub fn require_setup(app: &mut SolveApp) -> Option<String> {
    let name = app.manufacture_workspace.active_setup.clone();
    if !name.is_empty() && app.session.doc.cam.setup(&name).is_some() {
        return Some(name);
    }
    if let Some(setup) = app.session.doc.cam.setups.first() {
        app.manufacture_workspace.active_setup = setup.name.clone();
        return Some(setup.name.clone());
    }
    app.set_status(app.tr("manufacture.setup.create_first").to_string(), true);
    crate::manufacture_setup::open_new(app);
    None
}

pub fn document_tools(app: &SolveApp) -> Vec<Tool> {
    app.session
        .doc
        .cam
        .tools
        .iter()
        .map(|t| {
            let mut tool =
                Tool { number: t.number, diameter: t.diameter, flute_length: t.flute_length, description: t.description.clone(), ..Tool::default() };
            tool.flutes = t.flutes.min(64) as u8;
            tool.kind = match t.kind.label() {
                "ball end mill" => mfg_draft::ToolKind::BallEndmill,
                "drill" => mfg_draft::ToolKind::Drill,
                _ => mfg_draft::ToolKind::FlatEndmill,
            };
            if let Some(d) = t.shaft_diameter {
                tool.shaft_diameter = d;
            }
            if let Some(s) = t.stick_out {
                tool.length_below_holder = s;
            }
            tool
        })
        .collect()
}

pub fn load_operations(app: &SolveApp, setup_name: &str) -> Vec<Operation> {
    let Some(setup) = app.session.doc.cam.setup(setup_name) else {
        return Vec::new();
    };
    setup
        .operations
        .iter()
        .filter_map(|op| match op {
            CamOperation::Draft { spec, .. } => serde_json::from_value(spec.clone()).ok(),
            CamOperation::Contour { name, tool, .. } => Some(Operation::Profile(mfg_draft::ProfileOperation {
                name: name.clone(),
                tool_number: *tool,
                ..mfg_draft::ProfileOperation::default()
            })),
            CamOperation::Pocket { name, tool, .. } => Some(Operation::Pocket(mfg_draft::PocketOperation {
                name: name.clone(),
                tool_number: *tool,
                ..mfg_draft::PocketOperation::default()
            })),
        })
        .collect()
}

pub fn mfg_setup(app: &SolveApp, setup_name: &str) -> Option<MfgSetup> {
    let cam = app.session.doc.cam.setup(setup_name)?;
    let operation_type = serde_json::to_value(cam.operation_type).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    Some(MfgSetup {
        operations: load_operations(app, setup_name),
        tools: document_tools(app),
        machine: cam.machine.clone(),
        post: cam.post.clone(),
        operation_type,
        model_bodies: cam.model_bodies.clone(),
        ..MfgSetup::default()
    })
}

pub fn save_mfg_setup(app: &mut SolveApp, setup_name: &str, setup: &MfgSetup) -> bool {
    let Ok(operations) = serde_json::to_value(&setup.operations) else {
        app.set_status("could not write operations", true);
        return false;
    };
    let Ok(operation_type) = serde_json::to_value(setup.operation_type) else {
        app.set_status("could not write operation type", true);
        return false;
    };
    match app.run("cam.op.replace", json!({ "setup": setup_name, "operations": operations, "operation_type": operation_type })) {
        Ok(_) => true,
        Err(e) => {
            app.set_status(e, true);
            false
        }
    }
}

pub fn save_operations(app: &mut SolveApp, setup_name: &str, operations: &[Operation]) -> bool {
    let mut setup = mfg_setup(app, setup_name).unwrap_or_default();
    setup.operations = operations.to_vec();
    save_mfg_setup(app, setup_name, &setup)
}

pub fn setup_names(app: &SolveApp) -> Vec<String> {
    app.session.doc.cam.setups.iter().map(|s| s.name.clone()).collect()
}

pub fn named_posts(app: &SolveApp) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = app.session.doc.cam.posts.iter().map(|p| (p.name.clone(), p.name.clone())).collect();
    for name in ["rs274", "grbl", "mach3", "fanuc", "haas"] {
        if !v.iter().any(|(id, _)| id == name) {
            v.push((name.into(), name.into()));
        }
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[allow(dead_code)]
pub fn map_setup_operation_type(ot: SetupOperationType) -> mfg_draft::SetupOperationType {
    match ot {
        SetupOperationType::Milling => mfg_draft::SetupOperationType::Milling,
        SetupOperationType::Turning => mfg_draft::SetupOperationType::Turning,
        SetupOperationType::Cutting => mfg_draft::SetupOperationType::Cutting,
        SetupOperationType::Additive => mfg_draft::SetupOperationType::Additive,
    }
}
