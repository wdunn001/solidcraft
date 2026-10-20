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

use solidcraft_engine::io;

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
