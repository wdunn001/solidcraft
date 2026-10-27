//! The limits engine: every limit on a cut checked in one place, each answer naming the limit
//! that set it.
//!
//! For each cutting move, from the tool, the material, the machine, the spindle speed, the
//! feed and the move's [`Engagement`]:
//!
//! - feed per tooth `fz = vf / (n · z)` (mm);
//! - arc of contact for a cut `ae` wide, `φ = acos(1 − 2·ae/D)` (rad), and the mean chip
//!   thickness over it, `hm = fz · (1 − cos φ) / φ` (mm), from the chip thickness `fz · sin θ`
//!   at angle `θ` into the cut (Altintas, *Manufacturing Automation*, 2nd ed., 2012, ch. 2);
//! - specific cutting force `kc = kc11 · hm^(−mc)` (N/mm², Kienzle);
//! - cutting power `Pc = ap · ae · vf · kc / 60000` (W) and spindle torque `Mc = Pc / ω` (N·m),
//!   `ω = 2π·n/60` (Sandvik Coromant, milling formulas);
//! - peak tangential force on the tool, the force on one tooth at the thickest chip
//!   (`kc(h) · ap · h`, `h = fz` past a quarter turn of contact, else `fz · sin φ`) times the
//!   teeth in the cut at once (at least one) (N);
//! - tip deflection as a cantilever over the stick-out, `δ = F · L³ / (3 · E · I)`, with
//!   `I = π · de⁴ / 64` for an effective diameter `de = 0.8 · D` across the flutes. The 0.8 is a
//!   rule of thumb, (unverified), so the result says it is assumed.
//!
//! Every force-like quantity goes as `vf^(1 − mc)` (the power law of Kienzle's model), so the
//! feed that brings a quantity exactly to its limit is `vf · q^(−1 / (1 − mc))` for a quantity
//! `q` times its limit. That factor is the move's scale.
//!
//! Safety factor: the confidence setting `c` (0 conservative to 1 aggressive) gives a factor
//! of `2 − c` on every limit, and an input that is only assumed multiplies it by 1.25. Above
//! the factored limit a check warns; above the limit itself it stops, unless an input it rests
//! on is assumed. A check never stops on a guess.

use serde::{Deserialize, Serialize};

use crate::engage::{Engagement, EngagementKind};
use crate::gcode::Operation;
use crate::machine::Machine;
use crate::material::Material;
use crate::path::Move;
use crate::tool::{Source, ToolMaterial};

/// Default deflection limit when the tool gives none (mm).
pub const DEFAULT_MAX_DEFLECTION: f64 = 0.025;
/// A mean chip thinner than this rubs instead of cutting (mm). A typical edge-rounding scale,
/// (unverified) as a single figure, so it only ever warns.
pub const MIN_CHIP: f64 = 0.005;
/// Effective diameter across the flutes, as a fraction of the cutting diameter.
const FLUTED_DIAMETER: f64 = 0.8;
/// Extra factor on a limit whose inputs are only assumed.
const ASSUMED_FACTOR: f64 = 1.25;

/// A limit a cut can reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Limit {
    SpindlePower,
    SpindleTorque,
    SpindleSpeed,
    HolderSpeed,
    MachineFeed,
    ToolTorque,
    ToolForce,
    ToolDeflection,
    ChipThickness,
    RapidInMaterial,
}

impl Limit {
    pub fn label(self) -> &'static str {
        match self {
            Limit::SpindlePower => "spindle power",
            Limit::SpindleTorque => "spindle torque",
            Limit::SpindleSpeed => "spindle speed range",
            Limit::HolderSpeed => "holder speed",
            Limit::MachineFeed => "machine feed",
            Limit::ToolTorque => "tool torque",
            Limit::ToolForce => "tool force",
            Limit::ToolDeflection => "tool deflection",
            Limit::ChipThickness => "chip thickness",
            Limit::RapidInMaterial => "rapid move in material",
        }
    }

    fn unit(self) -> &'static str {
        match self {
            Limit::SpindlePower => "W",
            Limit::SpindleTorque | Limit::ToolTorque => "N·m",
            Limit::SpindleSpeed | Limit::HolderSpeed => "rev/min",
            Limit::MachineFeed => "mm/min",
            Limit::ToolForce => "N",
            Limit::ToolDeflection | Limit::ChipThickness | Limit::RapidInMaterial => "mm",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Ok,
    Warn,
    Stop,
}

/// What a check found at its worst move in one operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub operation: String,
    pub limit: Limit,
    pub severity: Severity,
    /// Index of the worst move in the operation's toolpath.
    pub move_index: usize,
    /// What the cut asks for and what the limit allows, in the limit's unit.
    pub demand: f64,
    pub allowed: f64,
    pub unit: String,
    /// The feed factor that brings this move to the factored limit (`None`: feed cannot fix it).
    pub scale: Option<f64>,
    /// The inputs that were only assumed.
    pub assumed: Vec<String>,
    /// Whether the operation's accepted findings let this one through.
    #[serde(default)]
    pub accepted: bool,
    /// One plain sentence for people.
    pub summary: String,
}

/// The limits on one operation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpReport {
    pub operation: String,
    /// Every limit over its factored value, worst move each.
    pub findings: Vec<Finding>,
    /// Smallest scale over all moves, and the limit that set it.
    pub scale: Option<f64>,
    pub binding: Option<Limit>,
    /// Per move: the feed scale its binding limit allows (`None`: nothing limits it, or it is
    /// not a cutting move).
    pub move_scales: Vec<Option<f64>>,
    /// Per move: whether that scale rests only on data that is not assumed.
    pub move_trusted: Vec<bool>,
    /// What was not checked, and why.
    pub notes: Vec<String>,
    /// Peaks over the operation, for display.
    pub peak_power: f64,
    pub peak_torque: f64,
    pub peak_force: f64,
    pub peak_deflection: f64,
}

impl OpReport {
    pub fn worst(&self) -> Severity {
        self.findings.iter().filter(|f| !f.accepted).map(|f| f.severity).max().unwrap_or(Severity::Ok)
    }
}

/// What the checks know about the setup.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub material: Option<&'a Material>,
    pub machine: Option<&'a Machine>,
    /// 0 conservative to 1 aggressive.
    pub confidence: f64,
    /// Where the stock the engagement was measured on came from.
    pub stock: Source,
}

/// The forces of one cutting move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Forces {
    pub fz: f64,
    pub hm: f64,
    pub kc: f64,
    pub power: f64,
    pub torque: f64,
    pub force: f64,
    pub deflection: f64,
}

/// Forces for a cut `ae` wide and `ap` deep at `feed` mm/min, `rpm`, with a tool of diameter
/// `d`, `flutes` teeth and stick-out `l` (mm), modulus `e` (N/mm²), in a material `(kc11, mc)`.
#[allow(clippy::too_many_arguments)]
pub fn forces(ae: f64, ap: f64, feed: f64, rpm: f64, d: f64, flutes: u32, l: f64, e: f64, kc11: f64, mc: f64) -> Option<Forces> {
    if !(ae > 0.0 && ap > 0.0 && feed > 0.0 && rpm > 0.0 && d > 0.0 && flutes > 0) {
        return None;
    }
    let z = f64::from(flutes);
    let fz = feed / (rpm * z);
    let phi = (1.0 - 2.0 * (ae / d).min(1.0)).clamp(-1.0, 1.0).acos();
    if !(phi > 0.0) {
        return None;
    }
    let hm = fz * (1.0 - phi.cos()) / phi;
    let kc_at = |h: f64| kc11 * h.max(1e-6).powf(-mc);
    let kc = kc_at(hm);
    let power = ap * ae * feed * kc / 60_000.0;
    let omega = std::f64::consts::TAU * rpm / 60.0;
    let torque = power / omega;
    let h_max = if phi >= std::f64::consts::FRAC_PI_2 { fz } else { fz * phi.sin() };
    let in_cut = (phi * z / std::f64::consts::TAU).ceil().max(1.0);
    let force = kc_at(h_max) * ap * h_max * in_cut;
    let de = FLUTED_DIAMETER * d;
    let inertia = std::f64::consts::PI * de.powi(4) / 64.0;
    let deflection = force * l.powi(3) / (3.0 * e * inertia);
    Some(Forces { fz, hm, kc, power, torque, force, deflection })
}

struct Check {
    limit: Limit,
    demand: f64,
    allowed: f64,
    assumed: Vec<&'static str>,
    /// How the quantity scales with feed: `Some(k)` for `vf^k`, `None` when feed has no effect.
    exponent: Option<f64>,
}

/// Checks one operation's moves against every limit.
pub fn check_operation(op: &Operation, engagement: &[Option<Engagement>], ctx: &Context) -> OpReport {
    let tool = &op.tool;
    let ph = &tool.physical;
    let confidence = if ctx.confidence.is_finite() { ctx.confidence.clamp(0.0, 1.0) } else { 0.5 };
    let factor = 2.0 - confidence;
    let mut rep = OpReport { operation: op.name.clone(), ..OpReport::default() };
    let rpm = op.spindle_rpm;
    let stock_assumed = ctx.stock == Source::Assumed;

    // Checks that feed cannot fix, once per operation.
    let mut fixed: Vec<Check> = Vec::new();
    if let Some(m) = ctx.machine {
        let a = if m.source == Source::Assumed { vec!["machine"] } else { vec![] };
        if let Some(hi) = m.max_rpm.filter(|hi| rpm > *hi) {
            fixed.push(Check { limit: Limit::SpindleSpeed, demand: rpm, allowed: hi, assumed: a.clone(), exponent: None });
        }
        if let Some(lo) = m.min_rpm.filter(|lo| rpm < *lo) {
            fixed.push(Check { limit: Limit::SpindleSpeed, demand: rpm, allowed: lo, assumed: a.clone(), exponent: None });
        }
    }
    if let Some(hi) = ph.holder.as_ref().and_then(|h| h.max_rpm).filter(|hi| rpm > *hi) {
        let a = if ph.source == Source::Assumed { vec!["holder"] } else { vec![] };
        fixed.push(Check { limit: Limit::HolderSpeed, demand: rpm, allowed: hi, assumed: a, exponent: None });
    }
    for c in fixed {
        record(&mut rep, &c, 0, factor, None);
    }

    let (kienzle, kienzle_source) = match ctx.material {
        Some(m) => {
            let (k, mc, s) = m.kienzle();
            (Some((k, mc)), s)
        }
        None => {
            rep.notes.push("no material set: cutting forces were not checked".into());
            (None, Source::Assumed)
        }
    };
    if ctx.machine.is_none() {
        rep.notes.push("no machine set: spindle power, torque and speed were not checked".into());
    }
    if stock_assumed {
        rep.notes.push("no stock box set: engagement was measured on a box around the toolpaths".into());
    }
    let tool_mat = ph.material.unwrap_or(ToolMaterial::Carbide);
    let (stick_out, stick_assumed) = match ph.stick_out {
        Some(l) => (l, ph.source == Source::Assumed),
        None => (tool.flute_length, true),
    };

    for (i, m) in op.path.moves.iter().enumerate() {
        let e = engagement.get(i).copied().flatten();
        let feed = match *m {
            Move::Feed { feed, .. } | Move::Arc { feed, .. } => Some(feed),
            _ => None,
        };
        let mut scale: Option<f64> = None;
        let mut trusted = true;
        if let (Move::Rapid { .. }, Some(e)) = (m, e) {
            let a = if stock_assumed { vec!["stock"] } else { vec![] };
            let c = Check { limit: Limit::RapidInMaterial, demand: e.ap, allowed: 0.0, assumed: a, exponent: None };
            record(&mut rep, &c, i, factor, None);
        }
        if let (Some(feed), Some(e)) = (feed, e.filter(|e| e.kind != EngagementKind::Rapid)) {
            let mut checks: Vec<Check> = Vec::new();
            if let Some(m) = ctx.machine.and_then(|m| m.max_feed.map(|f| (m, f))).filter(|(_, f)| feed > *f) {
                let a = if m.0.source == Source::Assumed { vec!["machine"] } else { vec![] };
                checks.push(Check { limit: Limit::MachineFeed, demand: feed, allowed: m.1, assumed: a, exponent: Some(1.0) });
            }
            if let Some((kc11, mc)) = kienzle {
                let mut base: Vec<&'static str> = Vec::new();
                if kienzle_source == Source::Assumed {
                    base.push("material cutting data");
                }
                if stock_assumed {
                    base.push("stock");
                }
                let (ae, ap) = if e.kind == EngagementKind::Plunge { (tool.diameter, e.ap) } else { (e.ae, e.ap) };
                if let Some(f) = forces(ae, ap, feed, rpm, tool.diameter, tool.flutes, stick_out, tool_mat.youngs_modulus(), kc11, mc) {
                    rep.peak_power = rep.peak_power.max(f.power);
                    rep.peak_torque = rep.peak_torque.max(f.torque);
                    rep.peak_force = rep.peak_force.max(f.force);
                    rep.peak_deflection = rep.peak_deflection.max(f.deflection);
                    let k = Some(1.0 - mc);
                    if let Some(mach) = ctx.machine {
                        let mut a = base.clone();
                        if mach.source == Source::Assumed {
                            a.push("machine");
                        }
                        match mach.spindle_at(rpm) {
                            Some((p, t)) => {
                                checks.push(Check { limit: Limit::SpindlePower, demand: f.power, allowed: p, assumed: a.clone(), exponent: k });
                                checks.push(Check { limit: Limit::SpindleTorque, demand: f.torque, allowed: t, assumed: a, exponent: k });
                            }
                            None if !mach.spindle.is_empty() => {
                                checks.push(Check { limit: Limit::SpindleSpeed, demand: rpm, allowed: 0.0, assumed: a, exponent: None });
                            }
                            None => {}
                        }
                    }
                    let lim = ph.limits.as_ref();
                    let lim_assumed = lim.is_none_or(|l| l.source == Source::Assumed);
                    let with = |extra: &[&'static str]| {
                        let mut a = base.clone();
                        a.extend_from_slice(extra);
                        a
                    };
                    if let Some(t) = lim.and_then(|l| l.max_torque) {
                        checks.push(Check {
                            limit: Limit::ToolTorque,
                            demand: f.torque,
                            allowed: t,
                            assumed: with(if lim_assumed { &["tool limits"] } else { &[] }),
                            exponent: k,
                        });
                    }
                    if let Some(fl) = lim.and_then(|l| l.max_force) {
                        checks.push(Check {
                            limit: Limit::ToolForce,
                            demand: f.force,
                            allowed: fl,
                            assumed: with(if lim_assumed { &["tool limits"] } else { &[] }),
                            exponent: k,
                        });
                    }
                    let (dmax, dmax_assumed) = match lim.and_then(|l| l.max_deflection) {
                        Some(d) => (d, lim_assumed),
                        None => (DEFAULT_MAX_DEFLECTION, true),
                    };
                    let mut a = with(&["effective fluted diameter"]);
                    if dmax_assumed {
                        a.push("deflection limit");
                    }
                    if stick_assumed {
                        a.push("stick-out");
                    }
                    if ph.material.is_none() {
                        a.push("tool material");
                    }
                    checks.push(Check { limit: Limit::ToolDeflection, demand: f.deflection, allowed: dmax, assumed: a, exponent: k });
                    if f.hm < MIN_CHIP {
                        let c =
                            Check { limit: Limit::ChipThickness, demand: f.hm, allowed: MIN_CHIP, assumed: with(&["minimum chip"]), exponent: None };
                        record_thin(&mut rep, &c, i);
                    }
                }
            }
            for c in &checks {
                let s = record(&mut rep, c, i, factor, Some(e));
                if let Some(s) = s
                    && scale.is_none_or(|x| s < x)
                {
                    scale = Some(s);
                    trusted = c.assumed.is_empty();
                    if rep.scale.is_none_or(|x| s < x) {
                        rep.scale = Some(s);
                        rep.binding = Some(c.limit);
                    }
                }
            }
        }
        rep.move_scales.push(scale);
        rep.move_trusted.push(trusted && scale.is_some());
    }
    rep
}

/// Factors a check, files it as a finding when it is over, and returns the feed scale it allows.
fn record(rep: &mut OpReport, c: &Check, i: usize, factor: f64, e: Option<Engagement>) -> Option<f64> {
    let assumed = !c.assumed.is_empty();
    let f = if assumed { factor * ASSUMED_FACTOR } else { factor };
    let (q, over_raw) = match c.limit {
        // Feed cannot fix these: they are over or not.
        Limit::SpindleSpeed | Limit::HolderSpeed | Limit::RapidInMaterial => (f64::INFINITY, true),
        _ if c.allowed > 0.0 => (c.demand * f / c.allowed, c.demand > c.allowed),
        _ => (f64::INFINITY, true),
    };
    let scale = match c.exponent {
        Some(k) if q.is_finite() && q > 0.0 && k > 0.0 => Some(q.powf(-1.0 / k)),
        _ => None,
    };
    // A hair of rounding over is not over: a feed scaled to the limit lands exactly on it.
    if q <= 1.0 + 1e-9 {
        return scale;
    }
    let severity = if over_raw && !assumed { Severity::Stop } else { Severity::Warn };
    let summary = summary(c, severity, scale, e);
    let finding = Finding {
        operation: rep.operation.clone(),
        limit: c.limit,
        severity,
        move_index: i,
        demand: c.demand,
        allowed: c.allowed,
        unit: c.limit.unit().into(),
        scale,
        assumed: c.assumed.iter().map(|s| s.to_string()).collect(),
        accepted: false,
        summary,
    };
    // Keep the worst move per limit.
    match rep.findings.iter_mut().find(|x| x.limit == c.limit) {
        Some(x) if (finding.severity, finding.demand / finding.allowed.max(1e-12)) > (x.severity, x.demand / x.allowed.max(1e-12)) => *x = finding,
        Some(_) => {}
        None => rep.findings.push(finding),
    }
    scale
}

fn record_thin(rep: &mut OpReport, c: &Check, i: usize) {
    if rep.findings.iter().any(|x| x.limit == Limit::ChipThickness) {
        return;
    }
    rep.findings.push(Finding {
        operation: rep.operation.clone(),
        limit: c.limit,
        severity: Severity::Warn,
        move_index: i,
        demand: c.demand,
        allowed: c.allowed,
        unit: c.limit.unit().into(),
        scale: None,
        assumed: c.assumed.iter().map(|s| s.to_string()).collect(),
        accepted: false,
        summary: format!(
            "the chip is {:.4} mm thick at move {i}, under {:.3} mm: the tool rubs instead of cutting; raise the feed or the width of cut",
            c.demand, c.allowed
        ),
    });
}

fn summary(c: &Check, severity: Severity, scale: Option<f64>, e: Option<Engagement>) -> String {
    let what = c.limit.label();
    let unit = c.limit.unit();
    let cut = e.map(|e| format!(" ({} cut, {:.2} mm wide, {:.2} mm deep)", kind_label(e.kind), e.ae, e.ap)).unwrap_or_default();
    let head = match c.limit {
        Limit::RapidInMaterial => format!("a rapid move goes {:.2} mm into material", c.demand),
        Limit::SpindleSpeed if c.allowed == 0.0 => format!("the spindle curve does not cover {:.0} rev/min", c.demand),
        Limit::SpindleSpeed if c.demand < c.allowed => format!("limited by {what}: {:.0} rev/min asked, the lowest is {:.0}", c.demand, c.allowed),
        Limit::SpindleSpeed | Limit::HolderSpeed => format!("limited by {what}: {:.0} rev/min asked, the highest is {:.0}", c.demand, c.allowed),
        _ => format!("limited by {what}: {:.3} {unit} asked, {:.3} {unit} allowed{cut}", c.demand, c.allowed),
    };
    let fix = match scale {
        Some(s) if s < 1.0 => format!("; feed at most {:.0}% of programmed", s * 100.0),
        _ => String::new(),
    };
    let trust = if c.assumed.is_empty() { String::new() } else { format!(" (assumed: {})", c.assumed.join(", ")) };
    let lead = if severity == Severity::Stop { "stop: " } else { "" };
    format!("{lead}{head}{fix}{trust}")
}

fn kind_label(k: EngagementKind) -> &'static str {
    match k {
        EngagementKind::Side => "side",
        EngagementKind::Slot => "slot",
        EngagementKind::Ramp => "ramp",
        EngagementKind::Plunge => "plunge",
        EngagementKind::Rapid => "rapid",
    }
}

/// Feeds scaled to what each move's binding limit allows: never below `min` or above `max`
/// times the programmed feed, never faster than programmed where the limit rests on an
/// assumption, and smoothed so neighbouring cutting moves differ by at most `ratio`. The
/// smoothing only ever lowers a feed, so no move ends above its limit.
pub fn adapt_feeds(op: &mut Operation, report: &OpReport, min: f64, max: f64, ratio: f64) {
    let n = op.path.moves.len();
    let mut feeds: Vec<Option<f64>> = op
        .path
        .moves
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let f = match *m {
                Move::Feed { feed, .. } | Move::Arc { feed, .. } => feed,
                _ => return None,
            };
            let s = report.move_scales.get(i).copied().flatten().unwrap_or(1.0);
            let trusted = report.move_trusted.get(i).copied().unwrap_or(false);
            let hi = if trusted { max } else { max.min(1.0) };
            Some(f * s.clamp(min, hi))
        })
        .collect();
    let ratio = ratio.max(1.0);
    for pass in 0..2 {
        let order: Box<dyn Iterator<Item = usize>> = if pass == 0 { Box::new(1..n) } else { Box::new((0..n.saturating_sub(1)).rev()) };
        for i in order {
            let j = if pass == 0 { i - 1 } else { i + 1 };
            if let (Some(Some(prev)), Some(Some(cur))) = (feeds.get(j).copied(), feeds.get(i).copied())
                && cur > prev * ratio
                && let Some(slot) = feeds.get_mut(i)
            {
                *slot = Some(prev * ratio);
            }
        }
    }
    for (m, f) in op.path.moves.iter_mut().zip(feeds) {
        match (m, f) {
            (Move::Feed { feed, .. } | Move::Arc { feed, .. }, Some(f)) if f.is_finite() && f > 0.0 => *feed = f,
            _ => {}
        }
    }
}
