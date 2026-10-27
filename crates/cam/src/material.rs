//! Workpiece materials: what the cutting force model needs to know about the stock.
//!
//! The force model is Kienzle's: the specific cutting force `kc = kc11 · h^(−mc)` (N/mm²) for a
//! mean chip thickness `h` (mm), where `kc11` is the force per square millimetre of chip at a
//! chip 1 mm thick and `mc` says how fast it rises as the chip gets thinner.

use serde::{Deserialize, Serialize};

use crate::tool::Source;
use crate::{Result, invalid};

/// The ISO 513 groups of workpiece material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsoGroup {
    /// Steel.
    P,
    /// Stainless steel.
    M,
    /// Cast iron.
    K,
    /// Non-ferrous metals (aluminium, copper alloys).
    N,
    /// Heat-resistant super alloys and titanium.
    S,
    /// Hardened steel.
    H,
}

impl IsoGroup {
    /// A generic `(kc11, mc)` for the group when the material gives none.
    ///
    /// These are round mid-range figures for each group, inside the commonly quoted overall
    /// range of kc11 from about 700 to 4000 N/mm² and mc from 0.2 to 0.3. The per-group figures
    /// are (unverified) against a primary table, so they are always [`Source::Assumed`] and a
    /// check built on them only warns. Real values come from a material library.
    pub fn generic_kienzle(self) -> (f64, f64) {
        match self {
            IsoGroup::P => (1700.0, 0.25),
            IsoGroup::M => (2100.0, 0.21),
            IsoGroup::K => (1100.0, 0.28),
            IsoGroup::N => (700.0, 0.25),
            IsoGroup::S => (2800.0, 0.25),
            IsoGroup::H => (3500.0, 0.25),
        }
    }
}

/// A workpiece material.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub group: IsoGroup,
    /// Specific cutting force at a 1 mm chip (N/mm²).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kc11: Option<f64>,
    /// Kienzle exponent (dimensionless).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mc: Option<f64>,
    /// Young's modulus (N/mm²), for stock stiffness later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub youngs_modulus: Option<f64>,
    /// Where `kc11` and `mc` came from.
    #[serde(default)]
    pub source: Source,
}

impl Material {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return invalid("a material name must be 1 to 128 characters");
        }
        if let Some(k) = self.kc11
            && !(100.0..=20_000.0).contains(&k)
        {
            return invalid("kc11 must be 100 to 20000 N/mm²");
        }
        if let Some(m) = self.mc
            && !(0.0..=0.9).contains(&m)
        {
            return invalid("mc must be 0 to 0.9");
        }
        if let Some(e) = self.youngs_modulus
            && (!(e > 0.0) || !(e <= 2.0e6))
        {
            return invalid("Young's modulus must be above 0 and at most 2000000 N/mm²");
        }
        Ok(())
    }

    /// `(kc11, mc, source)`: the material's own values when both are given, else the group's
    /// generic pair, marked assumed.
    pub fn kienzle(&self) -> (f64, f64, Source) {
        match (self.kc11, self.mc) {
            (Some(k), Some(m)) => (k, m, self.source),
            _ => {
                let (k, m) = self.group.generic_kienzle();
                (k, m, Source::Assumed)
            }
        }
    }
}
