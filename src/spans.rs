//! What a source entity's original geometry is made of, as an ordered list of spans.
//! Clean Up fits each span with segments; the "before" preview samples the same spans,
//! so the preview always shows the geometry Clean Up reads.

use crate::dxf_read::Geom;
use crate::geom::{bulge_is_straight, p, Seg, P};
use std::f64::consts::PI;

/// One stretch of an entity's original geometry described by one formula.
#[derive(Clone, Copy, Debug)]
pub enum Span<'a> {
    Line { a: P, b: P },
    /// CCW arc, angles in radians, `a0 < a1`.
    Arc { c: P, r: f64, a0: f64, a1: f64 },
    Circle { c: P, r: f64 },
    /// f(t) = c + u cos t + v sin t, for t in [t0, t1].
    Ellipse { c: P, u: P, v: P, t0: f64, t1: f64 },
    /// From one polyline vertex to the next: an arc with this bulge, straight when it is ~0.
    Bulge { a: P, b: P, bulge: f64 },
    /// One knot span of a NURBS spline, for t in [t0, t1].
    Nurbs { curve: Nurbs<'a>, span: usize, t0: f64, t1: f64 },
    /// The fit points of a spline that has no control points, joined in order.
    FitPoints(&'a [P]),
}

/// The spans of one entity, in order, and whether its end joins its start.
#[derive(Clone, Debug)]
pub struct Spans<'a> {
    pub spans: Vec<Span<'a>>,
    pub closed: bool,
}

impl Geom {
    pub fn spans(&self) -> Spans<'_> {
        let one = |span: Span<'static>, closed: bool| Spans { spans: vec![span], closed };
        match self {
            Geom::Line { a, b } => one(Span::Line { a: *a, b: *b }, false),
            Geom::Arc { c, r, a0, a1 } => one(Span::Arc { c: *c, r: *r, a0: *a0, a1: *a1 }, false),
            Geom::Circle { c, r } => one(Span::Circle { c: *c, r: *r }, true),
            Geom::Ellipse { c, u, v, t0, t1 } => {
                one(Span::Ellipse { c: *c, u: *u, v: *v, t0: *t0, t1: *t1 }, t1 - t0 >= FULL_TURN)
            }
            Geom::Poly { pts, bulges, closed } => Spans { spans: polyline_spans(pts, bulges, *closed), closed: *closed },
            Geom::Spline { degree, knots, ctrl, weights, fit, closed } => {
                let spans = if ctrl.is_empty() {
                    vec![Span::FitPoints(fit)]
                } else {
                    let curve = Nurbs { degree: *degree, knots, ctrl, weights };
                    curve.spans().into_iter().map(|(span, t0, t1)| Span::Nurbs { curve, span, t0, t1 }).collect()
                };
                Spans { spans, closed: *closed }
            }
        }
    }
}

impl Spans<'_> {
    /// Centre and radius when the entity is a full circle: a circle, a full arc, or a
    /// whole ellipse whose axes are equal and square.
    pub fn circle(&self) -> Option<(P, f64)> {
        match self.spans[..] {
            [Span::Circle { c, r }] => Some((c, r)),
            [Span::Arc { c, r, a0, a1 }] if a1 - a0 >= FULL_TURN => Some((c, r)),
            [Span::Ellipse { c, u, v, t0, t1 }] if t1 - t0 >= FULL_TURN => {
                let round = (u.len() - v.len()).abs() <= 1e-9 * u.len() && u.dot(v).abs() <= 1e-9 * u.len() * v.len();
                round.then_some((c, u.len()))
            }
            _ => None,
        }
    }
}

/// The spans of a polyline: one per vertex, plus the closing one when closed. The last
/// bulge belongs to the closing span, so an open polyline ignores it.
pub fn polyline_spans(pts: &[P], bulges: &[f64], closed: bool) -> Vec<Span<'static>> {
    let n = pts.len();
    let count = if closed { n } else { n.saturating_sub(1) };
    (0..count)
        .map(|k| Span::Bulge { a: pts[k], b: pts[(k + 1) % n], bulge: bulges.get(k).copied().unwrap_or(0.0) })
        .collect()
}

/// A sweep this close to a whole turn counts as a whole turn.
const FULL_TURN: f64 = 2.0 * PI - 1e-9;

/// What the "before" preview draws for one source entity.
#[derive(Clone, Debug)]
pub struct Outline {
    /// The entity's spans, sampled and joined in order (closed outlines end where they start).
    pub pts: Vec<P>,
    /// The points a cutter would treat as vertices.
    pub nodes: Vec<P>,
}

pub fn outline(g: &Geom) -> Outline {
    let spans = g.spans();
    let mut pts: Vec<P> = Vec::new();
    for s in &spans.spans {
        let smp = preview_samples(s);
        // a span usually starts where the previous one ended: draw that point once
        let joined = matches!((pts.last(), smp.first()), (Some(end), Some(start)) if end.dist(*start) <= CLOSING_GAP);
        pts.extend_from_slice(&smp[usize::from(joined)..]);
    }
    if spans.closed {
        if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
            if last.dist(first) > CLOSING_GAP {
                // Clean Up closes it with a straight line, so the preview does too
                pts.push(first);
            }
        }
    }
    let nodes = match g {
        Geom::Line { a, b } => vec![*a, *b],
        Geom::Arc { .. } => vec![pts[0], pts[pts.len() - 1]],
        Geom::Circle { .. } | Geom::Ellipse { .. } => vec![],
        Geom::Poly { pts: vertices, .. } => vertices.clone(),
        Geom::Spline { ctrl, fit, .. } if ctrl.is_empty() => fit.clone(),
        Geom::Spline { ctrl, .. } => {
            let starts = spans.spans.iter().filter_map(|s| match *s {
                Span::Nurbs { t0, .. } => Some(s.at(t0)),
                _ => None,
            });
            starts.chain(ctrl.iter().copied()).collect()
        }
    };
    Outline { pts, nodes }
}

/// Ends closer than this already look joined in the preview.
const CLOSING_GAP: f64 = 1e-6;

/// The points the preview draws for one span, from its start to its end.
fn preview_samples(s: &Span) -> Vec<P> {
    let along = |n: usize, t0: f64, t1: f64| (0..=n).map(|i| s.at(t0 + (t1 - t0) * i as f64 / n as f64)).collect();
    match *s {
        Span::Line { a, b } => vec![a, b],
        Span::Circle { .. } => (0..=96).map(|i| s.at(i as f64 / 96.0 * 2.0 * PI)).collect(),
        Span::Arc { a0, a1, .. } => along(64, a0, a1),
        Span::Ellipse { t0, t1, .. } => along(96, t0, t1),
        Span::Bulge { a, .. } => {
            let mut v = along(if s.is_straight() { 1 } else { 24 }, 0.0, 1.0);
            v[0] = a;
            v
        }
        Span::Nurbs { t0, t1, .. } => along(24, t0, t1),
        Span::FitPoints(f) => f.to_vec(),
    }
}

impl Span<'_> {
    /// A line, or a polyline span whose bulge is too small to bow.
    pub fn is_straight(&self) -> bool {
        match *self {
            Span::Line { .. } => true,
            Span::Bulge { bulge, .. } => bulge_is_straight(bulge),
            _ => false,
        }
    }

    /// The point at `t`: the angle on an arc or circle, the parameter of an ellipse or
    /// spline span, or the fraction (0 to 1) along a line or polyline span.
    /// Fit points have no parameter.
    pub fn at(&self, t: f64) -> P {
        match *self {
            Span::Line { a, b } => a.lerp(b, t),
            Span::Arc { c, r, .. } | Span::Circle { c, r } => c + p(t.cos(), t.sin()) * r,
            Span::Ellipse { c, u, v, .. } => c + u * t.cos() + v * t.sin(),
            Span::Bulge { a, b, bulge } => Seg::new(a, b, bulge).point_at(t),
            Span::Nurbs { curve, span, .. } => curve.eval_span(span, t),
            Span::FitPoints(_) => panic!("fit points have no parameter"),
        }
    }
}

/// A NURBS curve, evaluated one knot span at a time.
#[derive(Clone, Copy, Debug)]
pub struct Nurbs<'a> {
    pub degree: usize,
    pub knots: &'a [f64],
    pub ctrl: &'a [P],
    pub weights: &'a [f64],
}

impl Nurbs<'_> {
    /// Point at `t` within knot span `span` (de Boor).
    pub fn eval_span(&self, span: usize, t: f64) -> P {
        let deg = self.degree;
        let mut d: Vec<[f64; 3]> = (0..=deg)
            .map(|j| {
                let i = j + span - deg;
                let w = self.weights[i];
                [self.ctrl[i].x * w, self.ctrl[i].y * w, w]
            })
            .collect();
        for r in 1..=deg {
            for j in (r..=deg).rev() {
                let i0 = j + span - deg;
                let den = self.knots[j + 1 + span - r] - self.knots[i0];
                let alpha = if den.abs() < 1e-300 { 0.0 } else { (t - self.knots[i0]) / den };
                for c in 0..3 {
                    d[j][c] = (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c];
                }
            }
        }
        let h = d[deg];
        if h[2].abs() < 1e-300 { p(h[0], h[1]) } else { p(h[0] / h[2], h[1] / h[2]) }
    }

    /// The knot spans of non-zero length, as (span, t0, t1).
    fn spans(&self) -> Vec<(usize, f64, f64)> {
        let n = self.ctrl.len();
        (self.degree..n)
            .filter(|&s| self.knots[s + 1] > self.knots[s])
            .map(|s| (s, self.knots[s], self.knots[s + 1]))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(pts: &[(f64, f64)], bulges: &[f64], closed: bool) -> Geom {
        Geom::Poly { pts: pts.iter().map(|&(x, y)| p(x, y)).collect(), bulges: bulges.to_vec(), closed }
    }

    /// (a, b, bulge) of every span, which must all be polyline spans.
    fn bulges(s: &Spans) -> Vec<(P, P, f64)> {
        s.spans
            .iter()
            .map(|s| match *s {
                Span::Bulge { a, b, bulge } => (a, b, bulge),
                other => panic!("not a polyline span: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn closed_polyline_gets_its_closing_span_with_the_last_bulge() {
        let g = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], &[0.0, 0.5, -0.3], true);
        let s = g.spans();
        assert!(s.closed);
        assert_eq!(
            bulges(&s),
            vec![(p(0.0, 0.0), p(10.0, 0.0), 0.0), (p(10.0, 0.0), p(10.0, 10.0), 0.5), (p(10.0, 10.0), p(0.0, 0.0), -0.3)]
        );
    }

    #[test]
    fn open_polyline_has_no_closing_span_and_ignores_its_last_bulge() {
        let g = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], &[0.2, 0.5, 0.9], false);
        let s = g.spans();
        assert!(!s.closed);
        assert_eq!(bulges(&s), vec![(p(0.0, 0.0), p(10.0, 0.0), 0.2), (p(10.0, 0.0), p(10.0, 10.0), 0.5)]);
    }

    #[test]
    fn full_circles_whatever_entity_they_came_from() {
        let c = p(1.0, 2.0);
        assert_eq!(Geom::Circle { c, r: 3.0 }.spans().circle(), Some((c, 3.0)));
        assert_eq!(Geom::Arc { c, r: 3.0, a0: 1.0, a1: 1.0 + 2.0 * PI }.spans().circle(), Some((c, 3.0)));
        let round = Geom::Ellipse { c, u: p(0.0, 3.0), v: p(-3.0, 0.0), t0: 0.0, t1: 2.0 * PI };
        assert_eq!(round.spans().circle(), Some((c, 3.0)));
    }

    #[test]
    fn partial_arcs_and_oval_ellipses_are_not_circles() {
        let c = p(0.0, 0.0);
        assert_eq!(Geom::Arc { c, r: 3.0, a0: 0.0, a1: 6.0 }.spans().circle(), None);
        let oval = Geom::Ellipse { c, u: p(4.0, 0.0), v: p(0.0, 3.0), t0: 0.0, t1: 2.0 * PI };
        assert_eq!(oval.spans().circle(), None);
        assert!(oval.spans().closed, "a whole ellipse is closed");
        let part = Geom::Ellipse { c, u: p(3.0, 0.0), v: p(0.0, 3.0), t0: 0.0, t1: PI };
        assert_eq!(part.spans().circle(), None);
        assert!(!part.spans().closed);
    }

    #[test]
    fn spline_spans_skip_zero_length_knot_spans() {
        // two cubic Bezier spans: knots 0,0,0,0,1,1,1,2,2,2,2 -> spans [0,1] and [1,2] only
        let ctrl: Vec<P> = (0..7).map(|i| p(i as f64, (i % 2) as f64)).collect();
        let g = Geom::Spline {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0],
            ctrl: ctrl.clone(),
            weights: vec![1.0; 7],
            fit: vec![],
            closed: false,
        };
        let s = g.spans();
        let ts: Vec<(f64, f64)> = s
            .spans
            .iter()
            .map(|s| match *s {
                Span::Nurbs { t0, t1, .. } => (t0, t1),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(ts, vec![(0.0, 1.0), (1.0, 2.0)]);
        let Span::Nurbs { curve, span, t0, .. } = s.spans[0] else { unreachable!() };
        assert!(curve.eval_span(span, t0).dist(ctrl[0]) < 1e-12, "a clamped spline starts on its first control point");
    }

    fn bezier_pair() -> Geom {
        bezier(false)
    }

    /// Two cubic Bezier spans from (0, 0) via (4, 0) to (8, 0).
    fn bezier(closed: bool) -> Geom {
        Geom::Spline {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0],
            ctrl: vec![p(0.0, 0.0), p(1.0, 2.0), p(3.0, 2.0), p(4.0, 0.0), p(5.0, -2.0), p(7.0, -2.0), p(8.0, 0.0)],
            weights: vec![1.0; 7],
            fit: vec![],
            closed,
        }
    }

    #[test]
    fn closed_spline_preview_ends_where_it_starts() {
        // Clean Up closes these with a straight line; the preview must show the same outline
        let fit = vec![p(0.0, 0.0), p(10.0, 5.0), p(20.0, 0.0), p(10.0, -5.0), p(1.0, -0.5)];
        let g = Geom::Spline { degree: 3, knots: vec![], ctrl: vec![], weights: vec![], fit: fit.clone(), closed: true };
        let o = outline(&g);
        assert_eq!(o.pts.first(), o.pts.last());
        assert_eq!(o.pts[..fit.len()], fit[..]);
        // a closed control-point spline whose ends are 8 apart
        let o = outline(&bezier(true));
        assert!(o.pts[0].dist(p(0.0, 0.0)) < 1e-12);
        assert_eq!(o.pts.first(), o.pts.last());
    }

    #[test]
    fn open_outlines_are_not_closed_by_the_preview() {
        let o = outline(&bezier_pair());
        assert!(o.pts.first().unwrap().dist(*o.pts.last().unwrap()) > 1.0);
    }

    #[test]
    fn preview_passes_through_every_span_end() {
        let g = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], &[0.3, 0.4, 0.0, -0.7], true);
        let o = outline(&g);
        for s in g.spans().spans {
            let Span::Bulge { a, b, .. } = s else { unreachable!() };
            assert!(o.pts.iter().any(|q| q.dist(a) < 1e-9) && o.pts.iter().any(|q| q.dist(b) < 1e-9), "{a:?} {b:?}");
        }
        assert_eq!(o.pts[0], p(0.0, 0.0), "starts exactly on the first vertex");
        // the joint between the two Bezier spans
        let o = outline(&bezier_pair());
        assert!(o.pts.iter().any(|q| q.dist(p(4.0, 0.0)) < 1e-12));
    }

    #[test]
    fn preview_bridges_a_spline_broken_at_a_knot_from_end_to_start() {
        // inner knot repeated degree + 1 times: the second span starts at ctrl[4], not where
        // the first one ends (ctrl[3])
        let ctrl: Vec<P> = (0..8).map(|i| p(i as f64, if i < 4 { 0.0 } else { 5.0 })).collect();
        let g = Geom::Spline {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0],
            ctrl: ctrl.clone(),
            weights: vec![1.0; 8],
            fit: vec![],
            closed: false,
        };
        let pts = outline(&g).pts;
        let at = |q: P| pts.iter().position(|x| x.dist(q) < 1e-12);
        let (end, start) = (at(ctrl[3]).expect("first span's end"), at(ctrl[4]).expect("second span's start"));
        assert_eq!(start, end + 1, "the break is bridged by one straight step");
    }

    #[test]
    fn preview_nodes_are_the_vertices_staff_see_today() {
        let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let g = poly(&corners, &[0.0, 0.5, 0.0], true);
        assert_eq!(outline(&g).nodes, corners.iter().map(|&(x, y)| p(x, y)).collect::<Vec<_>>());
        let arc = outline(&Geom::Arc { c: p(0.0, 0.0), r: 2.0, a0: 0.0, a1: PI });
        assert_eq!(arc.nodes.len(), 2);
        assert!(arc.nodes[0].dist(p(2.0, 0.0)) < 1e-12 && arc.nodes[1].dist(p(-2.0, 0.0)) < 1e-12);
        assert!(outline(&Geom::Circle { c: p(0.0, 0.0), r: 2.0 }).nodes.is_empty());
        let el = Geom::Ellipse { c: p(0.0, 0.0), u: p(4.0, 0.0), v: p(0.0, 2.0), t0: 0.0, t1: 1.0 };
        assert!(outline(&el).nodes.is_empty());
        let line = outline(&Geom::Line { a: p(1.0, 1.0), b: p(2.0, 2.0) });
        assert_eq!(line.nodes, vec![p(1.0, 1.0), p(2.0, 2.0)]);
        // a spline: where each span starts, then every control point
        let Geom::Spline { ctrl, .. } = bezier_pair() else { unreachable!() };
        let nodes = outline(&bezier_pair()).nodes;
        assert_eq!(nodes.len(), 2 + ctrl.len());
        assert!(nodes[0].dist(p(0.0, 0.0)) < 1e-12 && nodes[1].dist(p(4.0, 0.0)) < 1e-12);
        assert_eq!(nodes[2..], ctrl[..]);
        // a spline drawn through fit points: every fit point
        let fit = vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0)];
        let g = Geom::Spline { degree: 3, knots: vec![], ctrl: vec![], weights: vec![], fit: fit.clone(), closed: false };
        assert_eq!(outline(&g).nodes, fit);
    }

    #[test]
    fn spline_without_control_points_is_its_fit_points() {
        let fit = vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0)];
        let g = Geom::Spline { degree: 3, knots: vec![], ctrl: vec![], weights: vec![], fit: fit.clone(), closed: true };
        let s = g.spans();
        assert!(s.closed);
        assert!(matches!(s.spans[..], [Span::FitPoints(f)] if f == &fit[..]));
    }
}
