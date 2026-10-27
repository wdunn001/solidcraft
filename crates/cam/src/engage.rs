//! Engagement: how much material the tool is in on each move.
//!
//! The stock is a height field over the setup's stock box: one column of material per square
//! cell, its top at the column's height. The tool is swept through it move by move. At each
//! step along a move the cells under the tool whose material stands above the tool's surface
//! are the ones it cuts; their spread across the direction of travel is the radial width of
//! cut, and the highest of them above the tool's tip is the axial depth. Then the tool takes
//! them down to its surface.
//!
//! Measuring the material instead of asking each generator what it meant to cut means every
//! path is judged the same way: contours, pockets, a hand-made path or a program read back
//! from G-code. Stock carried from one operation to the next is cut as the earlier ones left it.

use serde::{Deserialize, Serialize};
use solidcraft_geom::{Vec2, Vec3};

use crate::path::{Move, Toolpath, arc_sweep};
use crate::tool::{Source, Tool, ToolKind};
use crate::{CamError, Result, invalid};

/// Most cells in one stock.
pub const MAX_CELLS: usize = 4_000_000;
/// Most steps taken along one move.
const MAX_STEPS: usize = 200_000;
/// Material thinner than this over the tool's surface is not counted (mm).
const SKIN: f64 = 1e-4;

/// The stock box in work coordinates (mm).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StockBounds {
    pub min: Vec3,
    pub max: Vec3,
    /// Whether the box is the real stock or a stand-in made from the toolpaths.
    pub source: Source,
}

/// What kind of cut a move makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngagementKind {
    /// Along a wall, part of the tool's width in the material.
    Side,
    /// The whole width of the tool in the material.
    Slot,
    /// Down and along at once.
    Ramp,
    /// Straight down.
    Plunge,
    /// A rapid that touched material: never a planned cut.
    Rapid,
}

/// The material one move cuts, at its worst point along the move.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Engagement {
    /// Radial width of cut, across the direction of travel (mm).
    pub ae: f64,
    /// Axial depth of cut, from the tool's tip to the highest material it touches (mm).
    pub ap: f64,
    pub kind: EngagementKind,
}

/// A stock being cut.
#[derive(Clone, Debug)]
pub struct Stock {
    origin: Vec2,
    cell: f64,
    nx: usize,
    ny: usize,
    bottom: f64,
    heights: Vec<f64>,
    pub bounds: StockBounds,
}

impl Stock {
    /// A full box of material, in square cells about `cell` mm wide (made wider where the box
    /// would need more than [`MAX_CELLS`]).
    pub fn new(bounds: StockBounds, cell: f64) -> Result<Stock> {
        let (lo, hi) = (bounds.min, bounds.max);
        if !(lo.is_finite() && hi.is_finite()) || !(hi.x > lo.x && hi.y > lo.y && hi.z > lo.z) {
            return invalid("the stock box must have a positive size in X, Y and Z");
        }
        if !(cell > 0.0) || !cell.is_finite() {
            return invalid("the stock cell size must be above 0");
        }
        let (w, d) = (hi.x - lo.x, hi.y - lo.y);
        if w.max(d) > 1.0e6 {
            return Err(CamError::TooLarge("the stock box is larger than 1 km".into()));
        }
        let cell = cell.max((w * d / MAX_CELLS as f64).sqrt() * 1.0001);
        let (nx, ny) = ((w / cell).ceil().max(1.0) as usize, (d / cell).ceil().max(1.0) as usize);
        let n = nx.checked_mul(ny).filter(|n| *n <= MAX_CELLS * 2).ok_or_else(|| CamError::TooLarge("too many stock cells".into()))?;
        Ok(Stock { origin: Vec2::new(lo.x, lo.y), cell, nx, ny, bottom: lo.z, heights: vec![hi.z; n], bounds })
    }

    /// The cell width chosen (mm).
    pub fn cell(&self) -> f64 {
        self.cell
    }

    /// Top of the material at `p` (mm), or `None` outside the stock.
    pub fn height_at(&self, p: Vec2) -> Option<f64> {
        let (i, j) = self.index(p)?;
        self.heights.get(j * self.nx + i).copied()
    }

    fn index(&self, p: Vec2) -> Option<(usize, usize)> {
        let (fx, fy) = ((p.x - self.origin.x) / self.cell, (p.y - self.origin.y) / self.cell);
        if !(fx >= 0.0 && fy >= 0.0) {
            return None;
        }
        let (i, j) = (fx as usize, fy as usize);
        (i < self.nx && j < self.ny).then_some((i, j))
    }

    fn centre(&self, i: usize, j: usize) -> Vec2 {
        self.origin + Vec2::new((i as f64 + 0.5) * self.cell, (j as f64 + 0.5) * self.cell)
    }

    /// Cuts `path` with `tool` and returns what each move engaged (`None`: nothing).
    pub fn cut(&mut self, path: &Toolpath, tool: &Tool) -> Result<Vec<Option<Engagement>>> {
        tool.validate()?;
        let shape = Shape::of(tool);
        let mut out = Vec::with_capacity(path.moves.len());
        let mut pos: Option<Vec3> = None;
        for m in &path.moves {
            let Some(to) = m.target() else {
                out.push(None);
                continue;
            };
            let Some(from) = pos else {
                pos = Some(to);
                out.push(None);
                continue;
            };
            let samples = self.samples(m, from, to)?;
            let mut worst: Option<(f64, f64)> = None;
            for w in samples.windows(2) {
                let [a, b] = w else { continue };
                if let Some((ae, ap)) = self.step(&shape, *a, *b) {
                    worst = Some(worst.map_or((ae, ap), |(e, p)| (e.max(ae), p.max(ap))));
                }
            }
            out.push(worst.map(|(ae, ap)| {
                let along = Vec2::new(to.x - from.x, to.y - from.y).len();
                let kind = match m {
                    Move::Rapid { .. } => EngagementKind::Rapid,
                    _ if along <= 1e-9 => EngagementKind::Plunge,
                    _ if (to.z - from.z).abs() > 1e-9 => EngagementKind::Ramp,
                    _ if ae >= tool.diameter - self.cell * 1.5 => EngagementKind::Slot,
                    _ => EngagementKind::Side,
                };
                let ae = if kind == EngagementKind::Plunge { tool.diameter } else { ae.min(tool.diameter) };
                Engagement { ae, ap: ap.min(tool.flute_length), kind }
            }));
            pos = Some(to);
        }
        Ok(out)
    }

    /// Points along a move, at most half a cell apart, starting at `from`.
    fn samples(&self, m: &Move, from: Vec3, to: Vec3) -> Result<Vec<Vec3>> {
        let step = self.cell * 0.5;
        let pts = match *m {
            Move::Arc { center, ccw, .. } => {
                let start = Vec2::new(from.x, from.y) - center;
                let (r, sweep) = (start.len(), arc_sweep(from, to, center, ccw));
                let n = ((r * sweep.abs()).hypot(to.z - from.z) / step).ceil().max(1.0);
                let n = self.steps(n)?;
                (0..=n)
                    .map(|i| {
                        let t = i as f64 / n as f64;
                        (center + Vec2::from_angle(start.angle() + sweep * t) * r).extend(from.z + (to.z - from.z) * t)
                    })
                    .collect()
            }
            _ => {
                let n = self.steps((from.dist(to) / step).ceil().max(1.0))?;
                (0..=n).map(|i| from.lerp(to, i as f64 / n as f64)).collect()
            }
        };
        Ok(pts)
    }

    fn steps(&self, n: f64) -> Result<usize> {
        if n <= MAX_STEPS as f64 { Ok(n as usize) } else { Err(CamError::TooLarge(format!("a move needs more than {MAX_STEPS} engagement steps"))) }
    }

    /// The tool moving from `a` to `b`: measures the material it meets at `b`, then removes it.
    /// `(ae, ap)` when it met any.
    fn step(&mut self, shape: &Shape, a: Vec3, b: Vec3) -> Option<(f64, f64)> {
        let r = shape.radius;
        let dir = Vec2::new(b.x - a.x, b.y - a.y);
        let across = dir.normalized().map(|u| Vec2::new(-u.y, u.x));
        let centre = Vec2::new(b.x, b.y);
        let (i0, j0) = self.index_clamped(centre - Vec2::new(r, r));
        let (i1, j1) = self.index_clamped(centre + Vec2::new(r, r));
        let (mut lo, mut hi, mut ap) = (f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for j in j0..=j1 {
            for i in i0..=i1 {
                let c = self.centre(i, j);
                let d = c.dist(centre);
                let Some(surface) = shape.surface(d, b.z) else { continue };
                let Some(h) = self.heights.get_mut(j * self.nx + i) else { continue };
                if *h > surface + SKIN {
                    ap = ap.max(*h - b.z);
                    if let Some(n) = across {
                        let x = (c - centre).dot(n);
                        lo = lo.min(x);
                        hi = hi.max(x);
                    }
                    *h = surface.max(self.bottom);
                }
            }
        }
        if !ap.is_finite() {
            return None;
        }
        let ae = if lo.is_finite() { (hi - lo + self.cell).min(2.0 * r) } else { 2.0 * r };
        Some((ae, ap.max(0.0)))
    }

    fn index_clamped(&self, p: Vec2) -> (usize, usize) {
        let f = |v: f64, o: f64, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n.saturating_sub(1));
        (f(p.x, self.origin.x, self.nx), f(p.y, self.origin.y, self.ny))
    }
}

/// The tool's end as a surface of revolution.
struct Shape {
    radius: f64,
    corner: f64,
}

impl Shape {
    fn of(tool: &Tool) -> Shape {
        let radius = tool.radius();
        let corner = match tool.kind {
            ToolKind::BallEndmill => radius,
            _ => tool.physical.corner_radius.unwrap_or(0.0).clamp(0.0, radius),
        };
        Shape { radius, corner }
    }

    /// Height of the tool's surface at distance `d` from its axis, with its tip at `tip`.
    fn surface(&self, d: f64, tip: f64) -> Option<f64> {
        if d > self.radius {
            return None;
        }
        let flat = self.radius - self.corner;
        if d <= flat || self.corner <= 0.0 {
            return Some(tip);
        }
        let x = d - flat;
        Some(tip + self.corner - (self.corner * self.corner - x * x).max(0.0).sqrt())
    }
}
