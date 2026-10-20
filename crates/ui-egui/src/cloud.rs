//! SolidCraft's half of Open and Save against cloud storage.
//!
//! The pages themselves are `craft_ui::cloud`. Every Craft app offers the
//! same ones. What belongs here is what only SolidCraft can answer. A
//! document is a design. These extensions and these content types are what
//! that means.
//!
//! SolidCraft holds no provider credential and speaks no provider protocol.
//! Every call goes to the CloudCraft service, which is the single place a
//! provider is configured and permissions are decided.
//!
//! Nothing in this file opens a socket either. The transport half is a
//! [`CloudBackend`] the host application supplies, the same way it supplies
//! the file pickers in `Services`. The pages state a [`Job`], the host runs
//! it on a worker, and the answer arrives on a channel that [`pump`] drains
//! on a later frame.

// Re-exported so the host application, and a test, can name these types
// without depending on the crates they come from.
pub use craft_cloud::{Capability, Entry, EntryKind};
pub use craft_ui::cloud::{
    Account, Busy, CloudApp, CloudBackend, CloudFiles, Conflict, CopyFormat, Destination, DocumentKind, Event, Fault, Folder, Job, Link, Origin,
    Post, Source, UNAVAILABLE, available, browse, conflict_modal, destination_name, has_target, link, load_shared, open_entry, open_page, origin,
    parent_of, prefers_cloud, progress, pump, reload_active, save, save_as, save_page, sign_in, sign_out, start, who,
};

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Color32, Painter, Rect, Stroke, StrokeKind, pos2};
use solidcraft_engine::{Session, Visibility, doc::Document, io};

use crate::{SolveApp, documents};

/// Extensions SolidCraft opens from the cloud. The design container, the old
/// name it still reads, and the JSON form of a design.
const OPENS: [&str; 3] = [io::DESIGN_EXT, io::LEGACY_DESIGN_EXT, "json"];

/// Content types SolidCraft opens, matched against the start of what the
/// provider said. Read beside the extension because neither is reliable
/// alone. A WebDAV server hands back `application/octet-stream` for a
/// perfectly good design, and a file exported by a web application often has
/// a correct content type and no extension at all.
const MIMES: [&str; 2] = ["application/x-solidcraft", "application/json"];

/// What a SolidCraft document is, as a cloud file list needs to know.
pub const DESIGN: DocumentKind = DocumentKind { extensions: &OPENS, mimes: &MIMES, noun: "design", default_extension: io::DESIGN_EXT };

/// The design snapshot each cloud save in flight encoded.
///
/// Only SolidCraft knows what a version of a design is. The pages hold none
/// of these. A save that succeeds marks saved exactly the version that
/// reached the server ([`Session::mark_saved_as`]). An edit made while the
/// bytes were on the wire stays unsaved.
pub type Sent = HashMap<u64, (Arc<Document>, Visibility)>;

/// Whether SolidCraft can open this entry.
pub fn opens(entry: &Entry) -> bool {
    craft_ui::cloud::opens(entry, DESIGN)
}

/// A typed name as a design file name. Any folder separators are dropped and
/// a missing extension becomes `.solidcraft`.
pub fn design_name(typed: &str) -> String {
    craft_ui::cloud::document_name(typed, DESIGN)
}

/// The whole of the cloud state, for the control channel and for tests.
pub fn status(app: &SolveApp) -> serde_json::Value {
    craft_ui::cloud::report(app)
}

/// The host part of the service address, for the account menu's first line.
///
/// Read from the environment and compiled in nowhere, because the service is
/// deployment-specific. Somebody with an account on two deployments needs to
/// know which one a design is about to land in.
fn service_host() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        String::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        ["SOLIDCRAFT_CLOUD_URL", "CRAFT_CLOUD_URL", "COLLABCRAFT_URL"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok())
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .find(|value| !value.is_empty())
            .map(|value| value.trim_start_matches("https://").trim_start_matches("http://").to_owned())
            .unwrap_or_default()
    }
}

impl CloudApp for SolveApp {
    fn cloud(&self) -> &CloudFiles {
        &self.cloud
    }

    fn cloud_mut(&mut self) -> &mut CloudFiles {
        &mut self.cloud
    }

    fn kind(&self) -> DocumentKind {
        DESIGN
    }

    fn active(&self) -> Option<u64> {
        Some(documents::active_uid(self))
    }

    fn title(&self, uid: u64) -> String {
        documents::session_of(self, uid).map(|s| s.doc.name.clone()).unwrap_or_default()
    }

    fn open_bytes(&mut self, name: &str, bytes: Vec<u8>) -> Result<Option<u64>, String> {
        let mut doc = io::read_design(&bytes).map_err(|e| e.to_string())?;
        // A design never named by its user takes its file's name, as opening
        // from disk does.
        let name = name.trim();
        if doc.name.trim().is_empty() || doc.name.trim() == "Untitled" {
            let stem = std::path::Path::new(name).file_stem().map(|s| s.to_string_lossy().trim().to_owned()).unwrap_or_default();
            if !stem.is_empty() {
                stem.clone_into(&mut doc.name);
            }
        }
        let session = Session::new(doc);
        if documents::active_is_blank(self) {
            documents::adopt(self, session);
        } else {
            documents::open_tab(self, session)?;
        }
        Ok(Some(documents::active_uid(self)))
    }

    fn encode(&mut self, uid: u64, into: Destination<'_>) -> Result<Vec<u8>, String> {
        // The destination decides the format: a design opened from the cloud
        // as JSON goes back as JSON, everything else is the container.
        let json = into.extension().eq_ignore_ascii_case("json");
        let (bytes, snapshot) = {
            let s = documents::session_of(self, uid).ok_or("no design open")?;
            let bytes = if json {
                io::write_design(&s.design())
            } else {
                s.design_container(io::DesignCompression::Fast, true, false).map_err(|e| e.to_string())?
            };
            (bytes, (s.doc.clone(), s.visibility.clone()))
        };
        self.cloud_sent.insert(uid, snapshot);
        Ok(bytes)
    }

    fn close(&mut self, uid: u64) {
        if let Some(i) = documents::index_of(self, uid) {
            documents::close(self, i, true);
        }
    }

    fn mark_saved(&mut self, uid: u64) {
        let Some((doc, visibility)) = self.cloud_sent.remove(&uid) else { return };
        if let Some(s) = documents::session_of_mut(self, uid) {
            s.mark_saved_as(doc, visibility);
        }
    }

    fn drop_pending(&mut self, uid: u64) {
        self.cloud_sent.remove(&uid);
    }

    fn ready_to_save(&mut self) -> bool {
        // A save has to include the rename still being typed in the browser.
        if crate::browser::needs_capture(self, crate::browser::SAVE) {
            self.tree.capture_prompt = Some(crate::browser::SAVE.into());
            return false;
        }
        true
    }

    fn sharing(&mut self) -> &mut craft_ui::share::ShareDialog {
        &mut self.share
    }

    fn presence(&mut self) -> &mut craft_ui::presence::PresenceBar {
        &mut self.presence
    }

    fn paint_icon(&self, painter: &Painter, rect: Rect, kind: EntryKind, colour: Color32) {
        let r = rect.shrink(rect.width() * 0.16);
        if kind == EntryKind::Folder {
            let tab = Rect::from_min_max(r.min, pos2(r.min.x + r.width() * 0.45, r.min.y + r.height() * 0.3));
            painter.rect_filled(tab, 1.0, colour);
            let body = Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * 0.2), r.max);
            painter.rect_filled(body, 2.0, colour);
        } else {
            // A design: an outlined sheet with feature lines.
            painter.rect_stroke(r, 2.0, Stroke::new(1.3, colour), StrokeKind::Inside);
            for i in 1..=2 {
                let y = r.min.y + r.height() * (i as f32 / 3.0);
                painter.line_segment([pos2(r.min.x + 3.0, y), pos2(r.max.x - 3.0, y)], Stroke::new(1.0, colour));
            }
        }
    }

    fn open_url(&self, url: &str) {
        if let Some(open) = &self.services.open_url {
            open(url);
        }
    }

    fn say(&mut self, text: String) {
        self.set_status(text, false);
    }

    fn toast(&mut self, text: String) {
        self.set_status(text, false);
    }

    fn app_name(&self) -> &str {
        "SolidCraft"
    }

    fn tr(&self, text: &str) -> String {
        SolveApp::tr(self, text).to_owned()
    }

    fn service(&self) -> String {
        service_host()
    }

    fn save_note(&self) -> Option<String> {
        Some(CloudApp::tr(self, "The design is saved as a SolidCraft design (.solidcraft) in the folder above."))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Services;

    fn app() -> SolveApp {
        SolveApp::new(Session::default(), Services::default())
    }

    /// Cloud bytes open as a design and encoded bytes read back as the same
    /// document. A save that answers on a later frame marks saved exactly the
    /// version that went over the wire: an edit made in between stays unsaved.
    #[test]
    fn cloud_bytes_round_trip_and_a_late_answer_keeps_edits_dirty() {
        let mut a = app();
        a.run("solid.box", json!({"length": 10, "width": 10, "height": 10})).unwrap();
        let uid = crate::documents::active_uid(&a);
        assert!(a.session.is_dirty());
        let bytes = CloudApp::encode(&mut a, uid, Destination::new("part.solidcraft")).unwrap();
        CloudApp::mark_saved(&mut a, uid);
        assert!(!a.session.is_dirty(), "the saved version is the one encoded");
        // A second save is answered only after another edit landed.
        let _ = CloudApp::encode(&mut a, uid, Destination::new("part.solidcraft")).unwrap();
        a.run("solid.sphere", json!({"diameter": 4})).unwrap();
        CloudApp::mark_saved(&mut a, uid);
        assert!(a.session.is_dirty(), "the edit made while the bytes were on the wire stays unsaved");
        // The first bytes open as their own design, named after the file.
        let opened = CloudApp::open_bytes(&mut a, "part.solidcraft", bytes).unwrap().unwrap();
        assert_ne!(opened, uid);
        assert_eq!(CloudApp::title(&a, opened), "part");
        assert_eq!(a.session.doc.features.len(), 1, "the opened design holds the box alone");
        // An answer for a design that closed in the meantime changes nothing.
        CloudApp::close(&mut a, opened);
        assert!(crate::documents::index_of(&a, opened).is_none());
        CloudApp::mark_saved(&mut a, opened);
    }
}
