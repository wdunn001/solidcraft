//! Cutting tools.

use serde::{Deserialize, Serialize};

use crate::{Result, invalid};

/// Largest tool number a post processor is asked to write.
pub const MAX_TOOL_NUMBER: u32 = 9999;
/// Largest tool diameter (mm).
pub const MAX_TOOL_DIAMETER: f64 = 1000.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    FlatEndmill,
    BallEndmill,
    Drill,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            ToolKind::FlatEndmill => "flat end mill",
            ToolKind::BallEndmill => "ball end mill",
            ToolKind::Drill => "drill",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// The pocket the machine holds it in (`T` word).
    pub number: u32,
    pub kind: ToolKind,
    /// Cutting diameter (mm).
    pub diameter: f64,
    /// Length of the cutting edges (mm).
    pub flute_length: f64,
    #[serde(default)]
    pub description: String,
    /// Number of cutting edges.
    #[serde(default = "two")]
    pub flutes: u32,
    /// The cutting data operations start from: spindle speed (rev/min) and feeds (mm/min).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spindle_rpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plunge_feed: Option<f64>,
    /// The depth per pass and the distance between passes operations start from (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepdown: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepover: Option<f64>,
    /// The physical data the limits read (see [`crate::limits`]). All optional: a tool without
    /// it posts as before, and the checks fall back to conservative assumptions they name.
    #[serde(default, skip_serializing_if = "Physical::is_empty")]
    pub physical: Physical,
}

/// Where a value came from, so a result can say how far to trust it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Measured on this tool, machine or material.
    Measured,
    /// From a maker's catalogue or a shared library entry.
    Catalogue,
    /// A default or a guess. Checks built on it warn but never stop a post.
    #[default]
    Assumed,
}

/// What a tool's cutting part is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolMaterial {
    Hss,
    Carbide,
    Ceramic,
    Pcd,
    Cbn,
    Other,
}

impl ToolMaterial {
    /// Young's modulus (N/mm²) used for deflection when the tool gives none: handbook-typical
    /// values, so always [`Source::Assumed`].
    pub fn youngs_modulus(self) -> f64 {
        match self {
            ToolMaterial::Hss => 210_000.0,
            ToolMaterial::Carbide => 600_000.0,
            ToolMaterial::Ceramic => 380_000.0,
            ToolMaterial::Pcd => 800_000.0,
            ToolMaterial::Cbn => 680_000.0,
            ToolMaterial::Other => 210_000.0,
        }
    }
}

/// The holder a tool sits in.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Holder {
    #[serde(default)]
    pub name: String,
    /// From the spindle gauge line to the holder's nose (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gauge_length: Option<f64>,
    /// Diameter at the nose, what clears the part (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nose_diameter: Option<f64>,
    /// Fastest the holder may turn (rev/min).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rpm: Option<f64>,
}

/// Limits the tool itself sets. Each one left out is not checked against the tool (the
/// deflection check then uses a conservative default, see [`crate::limits`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolLimits {
    /// Most torque at the tool (N·m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_torque: Option<f64>,
    /// Most tangential cutting force (N).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_force: Option<f64>,
    /// Most deflection at the tip (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_deflection: Option<f64>,
    #[serde(default)]
    pub source: Source,
}

/// The shared library entry a shop tool was made from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CatalogueRef {
    pub library: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

/// A tool's physical data: the assembly it is part of, what it is made of, its limits, and the
/// state of this one tool in the shop.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Physical {
    /// Radius of the corner between the end and the side (mm); 0 is a sharp corner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shank_diameter: Option<f64>,
    /// How far the tool sticks out of its holder (mm): the length that bends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stick_out: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<ToolMaterial>,
    /// Helix angle of the flutes (rad).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helix_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<Holder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<ToolLimits>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue: Option<CatalogueRef>,
    /// Wear of this tool, 0 new to 1 worn out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear: Option<f64>,
    /// Measured runout of this tool in its holder (mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runout: Option<f64>,
    /// Where the geometry above (stick-out, shank, corner) came from.
    #[serde(default)]
    pub source: Source,
}

impl Physical {
    pub fn is_empty(&self) -> bool {
        *self == Physical::default()
    }
}

fn two() -> u32 {
    2
}

impl Tool {
    pub fn new(number: u32, kind: ToolKind, diameter: f64, flute_length: f64) -> Tool {
        Tool {
            number,
            kind,
            diameter,
            flute_length,
            description: String::new(),
            flutes: 2,
            spindle_rpm: None,
            feed: None,
            plunge_feed: None,
            stepdown: None,
            stepover: None,
            physical: Physical::default(),
        }
    }
    pub fn radius(&self) -> f64 {
        self.diameter * 0.5
    }
    pub fn validate(&self) -> Result<()> {
        if self.number > MAX_TOOL_NUMBER {
            return invalid(format!("tool number {} is above {MAX_TOOL_NUMBER}", self.number));
        }
        if !(self.diameter > 0.0) || !(self.diameter <= MAX_TOOL_DIAMETER) {
            return invalid("the tool diameter must be above 0 and at most 1000 mm");
        }
        if !(self.flute_length > 0.0) || !self.flute_length.is_finite() {
            return invalid("the tool's flute length must be above 0");
        }
        if !(1..=100).contains(&self.flutes) {
            return invalid("a tool has 1 to 100 flutes");
        }
        if [self.spindle_rpm, self.feed, self.plunge_feed, self.stepdown, self.stepover].iter().flatten().any(|v| !(*v > 0.0) || !(*v <= 1.0e6)) {
            return invalid("a tool's spindle speed, feeds, stepdown and stepover must be above 0");
        }
        self.validate_physical()
    }

    fn validate_physical(&self) -> Result<()> {
        let ph = &self.physical;
        let positive = |v: Option<f64>, what: &str| -> Result<()> {
            match v {
                Some(v) if !(v > 0.0) || !(v <= 1.0e6) => invalid(format!("the tool's {what} must be above 0")),
                _ => Ok(()),
            }
        };
        if let Some(r) = ph.corner_radius
            && (!(r >= 0.0) || !(r <= self.radius()))
        {
            return invalid("the corner radius must be 0 to the tool radius");
        }
        positive(ph.shank_diameter, "shank diameter")?;
        positive(ph.stick_out, "stick-out")?;
        if let Some(h) = ph.helix_angle
            && !(0.0..=1.4).contains(&h)
        {
            return invalid("the helix angle must be 0 to 80 degrees");
        }
        if let Some(h) = &ph.holder {
            if h.name.len() > 256 {
                return invalid("a holder name is at most 256 characters");
            }
            positive(h.gauge_length, "holder gauge length")?;
            positive(h.nose_diameter, "holder nose diameter")?;
            positive(h.max_rpm, "holder max rpm")?;
        }
        if let Some(l) = &ph.limits {
            positive(l.max_torque, "torque limit")?;
            positive(l.max_force, "force limit")?;
            positive(l.max_deflection, "deflection limit")?;
        }
        if let Some(c) = &ph.catalogue
            && (c.library.is_empty() || c.id.is_empty() || [&c.library, &c.id, &c.version].iter().any(|s| s.len() > 256))
        {
            return invalid("a catalogue reference needs a library and an id, each at most 256 characters");
        }
        if let Some(w) = ph.wear
            && !(0.0..=1.0).contains(&w)
        {
            return invalid("tool wear is 0 (new) to 1 (worn out)");
        }
        if let Some(r) = ph.runout
            && (!(r >= 0.0) || !(r < self.radius()))
        {
            return invalid("the runout must be 0 or more and under the tool radius");
        }
        Ok(())
    }
}
