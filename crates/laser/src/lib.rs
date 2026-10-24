//! Laser mark domain: machine profiles and LightBurn / LaserCraft handoff packages.
//!
//! Geometry plus profile metadata for operator import. Direct device I/O stays out
//! of this crate (LaserCraft owns USB/WebUSB and galvo list bytes). Profiles are
//! data: a controller family, a work area, and optional galvo field hints, the same
//! contract LaserCraft's DeviceProfile uses so a `.laser-job.json` can be opened there.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod package;
pub mod profile;

pub use package::{LaserJobMeta, LaserLayer, LaserPackage, Polyline2, write_ascii_dxf, write_svg};
pub use profile::{Controller, LaserProfile, ProfileId, builtin_profiles, profile};

use thiserror::Error;

/// Hard error from laser package or profile helpers.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum LaserError {
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, LaserError>;

pub(crate) fn invalid(msg: impl Into<String>) -> LaserError {
    LaserError::Invalid(msg.into())
}
