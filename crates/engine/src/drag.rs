//! Dragging a part with what is wired to it: a drawn schematic wire that ends
//! on a moved pin, and a board track that ends on a moved pad, keep that end
//! on it.
//!
//! Dependencies: `component.move` and `footprint.move` in `lib.rs` call this
//! when asked to drag; the desktop canvas calls it on its preview copy so the
//! drag looks the same while the pointer is down as after it lets go.
//!
//! Local rules: a schematic wire end that follows its pin keeps leaving the
//! pin in the direction it did, with a bend added where needed; a track end
//! that follows its pad moves straight, so its angle may change.
use pcbcraft_doc::{Id, Project};
use pcbcraft_geom::Point;

/// Two points this close are the same connection point.
const SAME: f64 = 1e-6;

/// Where each pin of component `id`, or each pad of footprint `id`, sits now,
/// with its net. Empty when `id` is neither.
pub fn anchors(p: &Project, id: Id) -> Vec<(Option<Point>, Option<Id>)> {
    if let Some(c) = p.component(id) {
        return c
            .pins
            .iter()
            .map(|pin| (c.pin_position(&pin.number), pin.net))
            .collect();
    }
    if let Some(f) = p.footprint(id) {
        return f
            .pads
            .iter()
            .map(|pad| (Some(f.pad_world_position(pad)), pad.net))
            .collect();
    }
    Vec::new()
}

/// After part `id` moved, bring along the wire or track ends that sat on its
/// pins or pads, given where they were (`before`, from [`anchors`]).
pub fn follow(p: &mut Project, id: Id, before: &[(Option<Point>, Option<Id>)]) {
    let after = anchors(p, id);
    let moves: Vec<(Point, Point, Option<Id>)> = before
        .iter()
        .zip(&after)
        .filter_map(|((from, net), (to, _))| match (from, to) {
            (Some(from), Some(to)) if from.distance(*to) > SAME => Some((*from, *to, *net)),
            _ => None,
        })
        .collect();
    if moves.is_empty() {
        return;
    }
    if let Some(sheet) = p.component(id).map(|c| c.sheet) {
        follow_wires(p, sheet, &moves);
    } else {
        follow_tracks(p, &moves);
    }
}

/// Drawn wires on `sheet` with an end on a moved pin take that end along.
fn follow_wires(p: &mut Project, sheet: Id, moves: &[(Point, Point, Option<Id>)]) {
    let on_page: Vec<Id> = p
        .drawn_wires
        .iter()
        .filter(|w| p.on_schematic_page(sheet, w.id))
        .map(|w| w.id)
        .collect();
    for w in p.drawn_wires.iter_mut().filter(|w| on_page.contains(&w.id)) {
        for first in [true, false] {
            let end = if first {
                w.points.first()
            } else {
                w.points.last()
            };
            let Some(&end) = end else {
                continue;
            };
            if let Some((_, to, _)) = moves.iter().find(|(from, _, _)| from.distance(end) < SAME) {
                w.points = reshape(&w.points, first, *to);
            }
        }
    }
}

/// The wire `points` with its first (or last) end moved to `to`: the leg
/// from that end keeps its direction, bending once where it has to.
fn reshape(points: &[Point], first: bool, to: Point) -> Vec<Point> {
    let mut pts: Vec<Point> = points.to_vec();
    if !first {
        pts.reverse();
    }
    if let (Some(&a), Some(&b)) = (pts.first(), pts.get(1)) {
        let level = (a.y - b.y).abs() < SAME;
        let bend = if level {
            Point::new(b.x, to.y)
        } else {
            Point::new(to.x, b.y)
        };
        if pts.len() == 2 {
            pts = vec![to, bend, b];
        } else {
            pts[0] = to;
            pts[1] = bend;
        }
    }
    pts.dedup_by(|a, b| a.distance(*b) < SAME);
    if !first {
        pts.reverse();
    }
    pts
}

/// Tracks with an end on a moved pad of the same net take that end along.
fn follow_tracks(p: &mut Project, moves: &[(Point, Point, Option<Id>)]) {
    for t in &mut p.board.tracks {
        for start in [true, false] {
            let end = if start { t.start } else { t.end };
            let Some((from, to, _)) = moves
                .iter()
                .find(|(from, _, net)| from.distance(end) < SAME && net.is_none_or(|n| n == t.net))
            else {
                continue;
            };
            let shift = Point::new((to.x - from.x) / 2.0, (to.y - from.y) / 2.0);
            if start {
                t.start = *to;
            } else {
                t.end = *to;
            }
            // An arc's middle moves half as far as the end that moved.
            t.arc = t.arc.map(|m| m.offset(shift));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_stub_keeps_leaving_its_pin_level() {
        let stub = [Point::new(0.0, 0.0), Point::new(5.0, 0.0)];
        assert_eq!(
            reshape(&stub, true, Point::new(0.0, 2.0)),
            vec![
                Point::new(0.0, 2.0),
                Point::new(5.0, 2.0),
                Point::new(5.0, 0.0)
            ]
        );
        // Sliding along its own line adds no bend.
        assert_eq!(
            reshape(&stub, true, Point::new(-1.0, 0.0)),
            vec![Point::new(-1.0, 0.0), Point::new(5.0, 0.0)]
        );
        // The far end of a longer wire moves only its own leg.
        let wire = [
            Point::new(0.0, 0.0),
            Point::new(0.0, 5.0),
            Point::new(5.0, 5.0),
        ];
        assert_eq!(
            reshape(&wire, false, Point::new(5.0, 8.0)),
            vec![
                Point::new(0.0, 0.0),
                Point::new(0.0, 8.0),
                Point::new(5.0, 8.0)
            ]
        );
    }
}
