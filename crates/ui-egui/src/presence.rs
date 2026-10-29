//! Where everybody else is working.
//!
//! One bubble per person, drawn at the geometry they are editing, in the colour their badge in
//! the title bar already uses. The colour comes from `craft_ui::presence::colours_for`, which
//! derives it from the peer's own identifier, so the bubble here and the badge there agree with
//! no state passed between them.
//!
//! # Why a bubble rather than a glow or a pulse
//!
//! A steady colour carries the identity. Animation carries nothing on top of that, and it costs
//! a repaint every frame for as long as anybody is connected, because an egui app already has to
//! call `request_repaint` while a collaboration link is live. Collabora paints a per-view colour
//! on the cursor and the selection with no animation, for the same reason.
//!
//! # Obscured bubbles
//!
//! A bubble behind the part is drawn faint, and one in front is drawn normally. Two states with
//! no mode to find. Whether it is behind is decided by the ordinary pick: the nearest face under
//! that screen position, compared against the distance to the point itself.
//!
//! # Off-screen peers
//!
//! Somebody working outside this view gets a wedge at the edge of the viewport pointing at them,
//! in their colour. A bubble outside the frustum would draw nothing at all, and that is the case
//! where "who is looking where" matters most.

use craft_ui::presence::colours_for;
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use serde_json::{Value, json};
use solidcraft_engine::geom::Vec3;
use solidcraft_engine::render::Camera;

/// The ephemeral kinds this module speaks. Latest-wins, never ordered, never journaled.
pub mod kind {
    /// Where a peer is looking. Payload is a serialised `Camera`.
    pub const CAMERA: &str = "camera";
    /// What a peer is editing. Payload is `{ "point": [x, y, z], "what": "Sketch 3" }`.
    pub const AT: &str = "at";
    /// A peer asking the room to look somewhere. Payload is `{ "point": [x, y, z] }`.
    pub const LOOK: &str = "look";
}

/// How many bubbles draw before the rest fold into a count.
///
/// Four, the same number `PresenceBar` shows badges before folding. Every peer's marker drawn at
/// once in a busy assembly costs more legibility than it buys.
pub const SHOWN: usize = 4;

/// Bubble geometry, in points.
const BUBBLE: f32 = 22.0;
/// How far inside the viewport an edge wedge sits.
const EDGE_INSET: f32 = 14.0;
/// Alpha applied to a bubble the part is in front of.
const OBSCURED_ALPHA: u8 = 70;

/// What one peer last told the room about itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Remote {
    /// The relay's identifier for this connection. The colour is derived from it.
    pub peer: String,
    /// A display name, never an identity.
    pub display: String,
    /// Where they are looking, when they are sharing it.
    pub camera: Option<Camera>,
    /// The world point they are editing, when the document can name one.
    pub at: Option<Vec3>,
    /// What they are editing, in words, for the tooltip.
    pub what: Option<String>,
}

impl Remote {
    /// The letter on the bubble.
    pub fn initial(&self) -> String {
        self.display.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".to_owned())
    }
}

/// Whose view this window is following, if anybody's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Watching {
    #[default]
    Nobody,
    /// Follow this peer until the local user moves the camera.
    Peer(String),
}

/// Everything this window knows about where the others are.
#[derive(Clone, Debug)]
pub struct Presence {
    /// One entry per other peer, in roster order so the drawing does not jump about.
    pub remotes: Vec<Remote>,
    pub watching: Watching,
    /// Whether this window tells the room where it is looking.
    pub share_camera: bool,
    /// Whether this window draws anybody else's markers.
    pub show_others: bool,
    /// Draw an obscured bubble at all. On by default, at [`OBSCURED_ALPHA`].
    pub show_obscured: bool,
    /// The camera last sent, so an unchanged one is not sent again.
    sent: Option<Camera>,
}

impl Default for Presence {
    fn default() -> Self {
        Presence { remotes: Vec::new(), watching: Watching::Nobody, share_camera: true, show_others: true, show_obscured: true, sent: None }
    }
}

/// One marker to draw, already placed on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub peer: String,
    pub initial: String,
    pub colour: Color32,
    /// Where it goes.
    pub at: Pos2,
    /// Which treatment it gets.
    pub placement: Placement,
    /// The tooltip line.
    pub label: String,
}

/// How a marker sits in the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    /// In the view, with nothing in front of it.
    Visible,
    /// In the view, behind the part.
    Obscured,
    /// Outside the view. A wedge at the edge, pointing `toward`.
    OffScreen { toward: Vec2 },
}

impl Presence {
    /// Record one ephemeral message from another peer.
    ///
    /// Unknown kinds are ignored, so a peer running a newer build can say more without this one
    /// having to understand it.
    pub fn observe(&mut self, peer: &str, display: &str, kind: &str, payload: &Value) {
        if !self.remotes.iter().any(|r| r.peer == peer) {
            self.remotes.push(Remote { peer: peer.to_owned(), display: display.to_owned(), ..Remote::default() });
        }
        let Some(slot) = self.remotes.iter_mut().find(|r| r.peer == peer) else { return };
        if !display.is_empty() {
            slot.display = display.to_owned();
        }
        match kind {
            kind::CAMERA => slot.camera = serde_json::from_value(payload.clone()).ok(),
            kind::AT => {
                slot.at = point_from(payload.get("point"));
                slot.what = payload.get("what").and_then(Value::as_str).map(str::to_owned);
            }
            _ => {}
        }
    }

    /// Drop a peer that left. Their marker goes with them.
    ///
    /// A marker left behind would show somebody looking at a part they are no longer in.
    pub fn forget(&mut self, peer: &str) {
        self.remotes.retain(|r| r.peer != peer);
        if self.watching == Watching::Peer(peer.to_owned()) {
            self.watching = Watching::Nobody;
        }
    }

    /// Keep only the peers still in the room, in the roster's order.
    pub fn retain_roster(&mut self, roster: &[(String, String)]) {
        self.remotes.retain(|r| roster.iter().any(|(id, _)| id == &r.peer));
        self.remotes.sort_by_key(|r| roster.iter().position(|(id, _)| id == &r.peer).unwrap_or(usize::MAX));
        if let Watching::Peer(peer) = &self.watching
            && !roster.iter().any(|(id, _)| id == peer)
        {
            self.watching = Watching::Nobody;
        }
    }

    /// The camera to send this frame, if any.
    ///
    /// `None` when sharing is off or the view has not moved. A camera resent every frame would
    /// put a message on the wire for every mouse delta, and latest-wins makes all but the last
    /// of them waste.
    pub fn camera_to_send(&mut self, now: &Camera) -> Option<Value> {
        if !self.share_camera {
            return None;
        }
        if self.sent.as_ref() == Some(now) {
            return None;
        }
        self.sent = Some(*now);
        serde_json::to_value(now).ok()
    }

    /// The camera this window should adopt, when it is following somebody.
    pub fn followed_camera(&self) -> Option<Camera> {
        let Watching::Peer(peer) = &self.watching else { return None };
        self.remotes.iter().find(|r| &r.peer == peer).and_then(|r| r.camera)
    }

    /// The markers to draw, and how many peers did not fit.
    ///
    /// Pure, so the placement rules are testable without a viewport. `obscured` answers whether
    /// the part is in front of a world point, which the caller decides with the ordinary pick.
    pub fn markers(&self, rect: Rect, to_screen: impl Fn(Vec3) -> Option<Pos2>, obscured: impl Fn(Vec3, Pos2) -> bool) -> (Vec<Marker>, usize) {
        if !self.show_others {
            return (Vec::new(), 0);
        }
        let ids: Vec<String> = self.remotes.iter().map(|r| r.peer.clone()).collect();
        let colours = colours_for(&ids);

        let mut out = Vec::new();
        for (remote, colour) in self.remotes.iter().zip(colours) {
            let Some(point) = remote.at else { continue };
            let label = match &remote.what {
                Some(what) => format!("{} is editing {what}", remote.display),
                None => format!("{} is here", remote.display),
            };
            let placement_and_pos = match to_screen(point) {
                Some(screen) if rect.contains(screen) => {
                    let placement = if obscured(point, screen) { Placement::Obscured } else { Placement::Visible };
                    Some((placement, screen))
                }
                // Behind the camera, or outside the frustum. Either way the wedge points the
                // way, which is more useful than drawing nothing.
                Some(screen) => Some(edge_wedge(rect, screen)),
                None => None,
            };
            let Some((placement, at)) = placement_and_pos else { continue };
            if placement == Placement::Obscured && !self.show_obscured {
                continue;
            }
            out.push(Marker { peer: remote.peer.clone(), initial: remote.initial(), colour, at, placement, label });
        }

        // Visible markers earn the room first. A wedge at the edge is the cheapest to lose.
        out.sort_by_key(|m| match m.placement {
            Placement::Visible => 0,
            Placement::Obscured => 1,
            Placement::OffScreen { .. } => 2,
        });
        let folded = out.len().saturating_sub(SHOWN);
        out.truncate(SHOWN);
        (out, folded)
    }

    /// Draw the markers. Answers with the peer whose bubble was clicked.
    ///
    /// A click is worth answering because a bubble is where "take me to their view" belongs.
    pub fn draw(&self, painter: &Painter, markers: &[Marker], folded: usize, rect: Rect, pointer: Option<Pos2>, clicked: bool) -> Option<String> {
        let mut hit = None;
        for marker in markers {
            let radius = BUBBLE * 0.5;
            let over = pointer.is_some_and(|p| p.distance(marker.at) <= radius);
            if over && clicked {
                hit = Some(marker.peer.clone());
            }
            match marker.placement {
                Placement::OffScreen { toward } => wedge(painter, marker.at, toward, fade(marker.colour, 200)),
                Placement::Obscured => bubble(painter, marker, fade(marker.colour, OBSCURED_ALPHA), OBSCURED_ALPHA),
                Placement::Visible => bubble(painter, marker, marker.colour, 255),
            }
            if over {
                painter.text(marker.at + vec2(0.0, BUBBLE), Align2::CENTER_TOP, &marker.label, FontId::proportional(12.0), Color32::WHITE);
            }
        }
        if folded > 0 {
            painter.text(
                pos2(rect.right() - EDGE_INSET, rect.top() + EDGE_INSET),
                Align2::RIGHT_TOP,
                format!("+{folded}"),
                FontId::proportional(12.0),
                Color32::from_gray(190),
            );
        }
        hit
    }
}

/// One circle with an initial in it.
fn bubble(painter: &Painter, marker: &Marker, fill: Color32, alpha: u8) {
    let radius = BUBBLE * 0.5;
    painter.circle_filled(marker.at, radius, fill);
    painter.circle_stroke(marker.at, radius, Stroke::new(1.5, fade(Color32::BLACK, alpha / 2)));
    painter.text(marker.at, Align2::CENTER_CENTER, &marker.initial, FontId::proportional(12.0), fade(Color32::BLACK, alpha));
}

/// A triangle at the viewport edge pointing the way to somebody outside it.
fn wedge(painter: &Painter, at: Pos2, toward: Vec2, fill: Color32) {
    let size = BUBBLE * 0.45;
    let side = vec2(-toward.y, toward.x) * size * 0.6;
    painter.add(egui::Shape::convex_polygon(vec![at + toward * size, at - side, at + side], fill, Stroke::NONE));
}

/// Where a point outside the viewport is marked, and which way the wedge points.
///
/// The marker sits on the edge nearest the point, inset so the whole wedge stays drawn.
fn edge_wedge(rect: Rect, screen: Pos2) -> (Placement, Pos2) {
    let inner = rect.shrink(EDGE_INSET);
    let clamped = pos2(screen.x.clamp(inner.left(), inner.right()), screen.y.clamp(inner.top(), inner.bottom()));
    let toward = (screen - clamped).normalized();
    let toward = if toward.length() > 0.0 { toward } else { vec2(1.0, 0.0) };
    (Placement::OffScreen { toward }, clamped)
}

fn fade(colour: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha)
}

/// A world point out of an ephemeral payload.
fn point_from(value: Option<&Value>) -> Option<Vec3> {
    let array = value?.as_array()?;
    let [x, y, z] = array.as_slice() else { return None };
    Some(Vec3::new(x.as_f64()?, y.as_f64()?, z.as_f64()?))
}

/// The payload for "I am editing here".
pub fn at_payload(point: Vec3, what: Option<&str>) -> Value {
    json!({ "point": [point.x, point.y, point.z], "what": what })
}

/// The payload for "everybody look here".
pub fn look_payload(point: Vec3) -> Value {
    json!({ "point": [point.x, point.y, point.z] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))
    }

    fn with(peers: &[(&str, &str, Vec3)]) -> Presence {
        let mut p = Presence::default();
        for (id, name, at) in peers {
            p.observe(id, name, kind::AT, &at_payload(*at, Some("Sketch 1")));
        }
        p
    }

    /// A bubble takes the colour the title-bar badge takes, because both derive it from the
    /// peer id. Nothing passes a colour between them.
    #[test]
    fn a_bubble_takes_the_peers_own_colour() {
        let p = with(&[("peer-a", "Alice", Vec3::ZERO), ("peer-b", "Bob", Vec3::ZERO)]);
        let ids = vec!["peer-a".to_owned(), "peer-b".to_owned()];
        let expected = colours_for(&ids);
        let (markers, _) = p.markers(rect(), |_| Some(pos2(400.0, 300.0)), |_, _| false);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].colour, expected[0]);
        assert_eq!(markers[1].colour, expected[1]);
        assert_ne!(markers[0].colour, markers[1].colour, "two people in one room are told apart");
    }

    #[test]
    fn a_bubble_behind_the_part_is_obscured_and_still_drawn() {
        let p = with(&[("peer-a", "Alice", Vec3::ZERO)]);
        let (markers, _) = p.markers(rect(), |_| Some(pos2(400.0, 300.0)), |_, _| true);
        assert_eq!(markers[0].placement, Placement::Obscured);
    }

    #[test]
    fn an_obscured_bubble_can_be_switched_off() {
        let p = Presence { show_obscured: false, ..with(&[("peer-a", "Alice", Vec3::ZERO)]) };
        let (markers, _) = p.markers(rect(), |_| Some(pos2(400.0, 300.0)), |_, _| true);
        assert!(markers.is_empty());
    }

    /// Somebody outside the view gets a wedge at the nearest edge, pointing at them.
    #[test]
    fn an_off_screen_peer_gets_an_edge_wedge() {
        let p = with(&[("peer-a", "Alice", Vec3::ZERO)]);
        let (markers, _) = p.markers(rect(), |_| Some(pos2(2000.0, 300.0)), |_, _| false);
        assert_eq!(markers.len(), 1);
        let Placement::OffScreen { toward } = markers[0].placement else { panic!("expected a wedge") };
        assert!(toward.x > 0.9, "it points to the right, where the peer is");
        assert!(rect().contains(markers[0].at), "and the wedge itself stays drawn");
    }

    /// Past four, the rest become a count. Visible markers keep their place first.
    #[test]
    fn beyond_four_peers_the_rest_fold_into_a_count() {
        let mut p = Presence::default();
        for i in 0..7 {
            p.observe(&format!("peer-{i}"), &format!("P{i}"), kind::AT, &at_payload(Vec3::ZERO, None));
        }
        let (markers, folded) = p.markers(rect(), |_| Some(pos2(400.0, 300.0)), |_, _| false);
        assert_eq!(markers.len(), SHOWN);
        assert_eq!(folded, 3);
    }

    #[test]
    fn a_visible_marker_outranks_a_wedge_for_the_space() {
        let mut p = Presence::default();
        for i in 0..5 {
            p.observe(&format!("peer-{i}"), &format!("P{i}"), kind::AT, &at_payload(Vec3::new(i as f64, 0.0, 0.0), None));
        }
        // Only peer-0 is in the view. Everybody else is off to the right.
        let (markers, _) = p.markers(rect(), |v| Some(if v.x == 0.0 { pos2(400.0, 300.0) } else { pos2(2000.0, 300.0) }), |_, _| false);
        assert_eq!(markers[0].placement, Placement::Visible);
    }

    #[test]
    fn a_camera_is_sent_once_until_the_view_moves() {
        let mut p = Presence::default();
        let cam = Camera::default();
        assert!(p.camera_to_send(&cam).is_some(), "the first camera goes out");
        assert!(p.camera_to_send(&cam).is_none(), "an unmoved view sends nothing");
        let moved = Camera { distance: 400.0, ..cam };
        assert!(p.camera_to_send(&moved).is_some());
    }

    #[test]
    fn sharing_off_sends_no_camera() {
        let mut p = Presence { share_camera: false, ..Presence::default() };
        assert!(p.camera_to_send(&Camera::default()).is_none());
    }

    #[test]
    fn watching_adopts_that_peers_camera() {
        let mut p = with(&[("peer-a", "Alice", Vec3::ZERO)]);
        let cam = Camera { distance: 321.0, ..Camera::default() };
        p.observe("peer-a", "Alice", kind::CAMERA, &serde_json::to_value(cam).expect("camera serialises"));
        assert_eq!(p.followed_camera(), None, "following nobody adopts nothing");
        p.watching = Watching::Peer("peer-a".to_owned());
        assert_eq!(p.followed_camera().map(|c| c.distance), Some(321.0));
    }

    /// A peer who leaves takes their marker with them, and stops being followed.
    #[test]
    fn a_departed_peer_leaves_no_marker_behind() {
        let mut p = with(&[("peer-a", "Alice", Vec3::ZERO), ("peer-b", "Bob", Vec3::ZERO)]);
        p.watching = Watching::Peer("peer-a".to_owned());
        p.forget("peer-a");
        assert_eq!(p.remotes.len(), 1);
        assert_eq!(p.watching, Watching::Nobody);
    }

    #[test]
    fn the_roster_decides_who_is_still_here_and_in_what_order() {
        let mut p = with(&[("peer-a", "Alice", Vec3::ZERO), ("peer-b", "Bob", Vec3::ZERO)]);
        p.retain_roster(&[("peer-b".to_owned(), "Bob".to_owned())]);
        assert_eq!(p.remotes.len(), 1);
        assert_eq!(p.remotes[0].peer, "peer-b");
    }

    #[test]
    fn an_unknown_ephemeral_kind_is_ignored() {
        let mut p = Presence::default();
        p.observe("peer-a", "Alice", "something-newer", &json!({"x": 1}));
        assert_eq!(p.remotes.len(), 1, "the peer is still known");
        assert_eq!(p.remotes[0].at, None);
    }

    #[test]
    fn a_malformed_point_is_dropped_rather_than_guessed() {
        let mut p = Presence::default();
        p.observe("peer-a", "Alice", kind::AT, &json!({"point": [1.0, 2.0]}));
        assert_eq!(p.remotes[0].at, None);
    }
}
