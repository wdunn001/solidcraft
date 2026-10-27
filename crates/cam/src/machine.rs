//! Machines: what the spindle and the axes can give.

use serde::{Deserialize, Serialize};

use crate::tool::Source;
use crate::{Result, invalid};

/// Most points on a spindle curve.
pub const MAX_SPINDLE_POINTS: usize = 64;

/// One point of the spindle's curve: what it gives at one speed.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpindlePoint {
    /// rev/min
    pub rpm: f64,
    /// Power at the spindle (W).
    pub power: f64,
    /// Torque at the spindle (N·m).
    pub torque: f64,
}

/// A machine as the limits see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    /// rev/min
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_rpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rpm: Option<f64>,
    /// The spindle's power and torque against speed, in rising speed. Between points the
    /// curve is straight; outside them the spindle is not used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spindle: Vec<SpindlePoint>,
    /// Fastest cutting feed (mm/min).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_feed: Option<f64>,
    /// Most thrust each linear axis can push with (N), X, Y, Z.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_thrust: Option<[f64; 3]>,
    #[serde(default)]
    pub source: Source,
}

impl Machine {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return invalid("a machine name must be 1 to 128 characters");
        }
        let speed = |v: Option<f64>| v.is_none_or(|v| v > 0.0 && v <= 1.0e6);
        if !speed(self.min_rpm) || !speed(self.max_rpm) || !speed(self.max_feed) {
            return invalid("a machine's speeds and feed must be above 0");
        }
        if let (Some(lo), Some(hi)) = (self.min_rpm, self.max_rpm)
            && lo > hi
        {
            return invalid("the machine's lowest spindle speed is above its highest");
        }
        if self.spindle.len() > MAX_SPINDLE_POINTS {
            return invalid(format!("a spindle curve has at most {MAX_SPINDLE_POINTS} points"));
        }
        let mut last = 0.0;
        for p in &self.spindle {
            if !(p.rpm > last) || !(p.rpm <= 1.0e6) {
                return invalid("the spindle curve's speeds must be above 0 and rising");
            }
            if !(p.power > 0.0) || !(p.power <= 1.0e7) || !(p.torque > 0.0) || !(p.torque <= 1.0e6) {
                return invalid("the spindle curve's power and torque must be above 0");
            }
            last = p.rpm;
        }
        if let Some(t) = self.axis_thrust
            && !t.iter().all(|v| *v > 0.0 && *v <= 1.0e7)
        {
            return invalid("axis thrust must be above 0");
        }
        Ok(())
    }

    /// Power (W) and torque (N·m) the spindle gives at `rpm`; `None` outside its curve or
    /// without one.
    pub fn spindle_at(&self, rpm: f64) -> Option<(f64, f64)> {
        let (first, last) = (self.spindle.first()?, self.spindle.last()?);
        if !(rpm >= first.rpm) || !(rpm <= last.rpm) {
            return None;
        }
        let k = self.spindle.windows(2).find(|w| w.get(1).is_some_and(|b| rpm <= b.rpm));
        match k {
            Some([a, b]) => {
                let t = (rpm - a.rpm) / (b.rpm - a.rpm);
                Some((a.power + (b.power - a.power) * t, a.torque + (b.torque - a.torque) * t))
            }
            _ => Some((first.power, first.torque)),
        }
    }
}
