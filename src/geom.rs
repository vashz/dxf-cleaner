//! 2D geometry primitives: points, line/arc segments (bulge form), distances.

use std::f64::consts::PI;
use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

pub const fn p(x: f64, y: f64) -> P {
    P { x, y }
}

impl Add for P {
    type Output = P;
    fn add(self, o: P) -> P {
        p(self.x + o.x, self.y + o.y)
    }
}
impl Sub for P {
    type Output = P;
    fn sub(self, o: P) -> P {
        p(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for P {
    type Output = P;
    fn mul(self, s: f64) -> P {
        p(self.x * s, self.y * s)
    }
}
impl Neg for P {
    type Output = P;
    fn neg(self) -> P {
        p(-self.x, -self.y)
    }
}

impl P {
    pub fn dot(self, o: P) -> f64 {
        self.x * o.x + self.y * o.y
    }
    pub fn cross(self, o: P) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: P) -> f64 {
        (self - o).len()
    }
    /// Left-hand perpendicular (rotated +90°).
    pub fn perp(self) -> P {
        p(-self.y, self.x)
    }
    pub fn norm(self) -> P {
        let l = self.len();
        if l > 0.0 { self * (1.0 / l) } else { self }
    }
    pub fn lerp(self, o: P, t: f64) -> P {
        self + (o - self) * t
    }
    pub fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Signed angle from `a` to `b` (radians, CCW positive).
pub fn signed_angle(a: P, b: P) -> f64 {
    a.cross(b).atan2(a.dot(b))
}

pub fn dist_point_segment(q: P, a: P, b: P) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    if l2 <= 0.0 {
        return q.dist(a);
    }
    let t = ((q - a).dot(ab) / l2).clamp(0.0, 1.0);
    q.dist(a + ab * t)
}

/// Bulge of the arc a -> m -> b passing through `m` (m between a and b along the arc).
pub fn bulge_through(a: P, m: P, b: P) -> f64 {
    let va = a - m;
    let vb = b - m;
    if va.len() == 0.0 || vb.len() == 0.0 {
        return 0.0;
    }
    let inscribed = va.cross(vb).abs().atan2(va.dot(vb)); // angle a-m-b in [0, pi]
    let sweep = 2.0 * (PI - inscribed);
    let side = (m - a).cross(b - a); // > 0: m on the right of a->b => counter-clockwise arc
    let sign = if side > 0.0 { 1.0 } else { -1.0 };
    sign * (sweep / 4.0).tan()
}

/// Bulge of the arc from a to b whose start tangent is `t`.
pub fn bulge_from_start_tangent(a: P, b: P, t: P) -> f64 {
    let half = signed_angle(t, b - a);
    (half / 2.0).tan()
}

/// Bulge of the arc from a to b whose end tangent is `t`.
pub fn bulge_from_end_tangent(a: P, b: P, t: P) -> f64 {
    let half = signed_angle(b - a, t);
    (half / 2.0).tan()
}

/// A line (bulge = 0) or circular arc from `a` to `b`.
/// `bulge = tan(sweep/4)`, positive = counter-clockwise.
#[derive(Clone, Copy, Debug)]
pub struct Seg {
    pub a: P,
    pub b: P,
    pub bulge: f64,
}

pub const LINE_BULGE_EPS: f64 = 1e-9;

/// A bulge this small draws a straight line.
pub fn bulge_is_straight(bulge: f64) -> bool {
    bulge.abs() < LINE_BULGE_EPS
}

#[derive(Clone, Copy, Debug)]
pub struct ArcInfo {
    pub c: P,
    pub r: f64,
    /// Start angle (radians).
    pub a0: f64,
    /// Signed sweep (radians), CCW positive.
    pub sweep: f64,
}

impl Seg {
    pub fn new(a: P, b: P, bulge: f64) -> Seg {
        Seg { a, b, bulge }
    }
    pub fn is_line(&self) -> bool {
        bulge_is_straight(self.bulge)
    }
    pub fn chord(&self) -> f64 {
        self.a.dist(self.b)
    }
    pub fn sweep(&self) -> f64 {
        4.0 * self.bulge.atan()
    }
    /// Max distance between the arc and its chord.
    pub fn sagitta(&self) -> f64 {
        (self.bulge * self.chord() / 2.0).abs()
    }
    pub fn arc(&self) -> Option<ArcInfo> {
        if self.is_line() {
            return None;
        }
        let c = self.chord();
        if c <= 0.0 {
            return None;
        }
        let u = (self.b - self.a) * (1.0 / c);
        let n = u.perp();
        let m = self.a.lerp(self.b, 0.5);
        let h = (c / 2.0) * (1.0 - self.bulge * self.bulge) / (2.0 * self.bulge);
        let center = m + n * h;
        let r = center.dist(self.a);
        Some(ArcInfo { c: center, r, a0: (self.a - center).angle(), sweep: self.sweep() })
    }
    pub fn length(&self) -> f64 {
        match self.arc() {
            Some(ai) => ai.r * ai.sweep.abs(),
            None => self.chord(),
        }
    }
    pub fn point_at(&self, t: f64) -> P {
        match self.arc() {
            Some(ai) => {
                let ang = ai.a0 + ai.sweep * t;
                ai.c + p(ang.cos(), ang.sin()) * ai.r
            }
            None => self.a.lerp(self.b, t),
        }
    }
    pub fn midpoint(&self) -> P {
        self.point_at(0.5)
    }
    /// `n+1` points from a to b inclusive.
    pub fn samples(&self, n: usize) -> Vec<P> {
        let n = n.max(1);
        (0..=n).map(|i| self.point_at(i as f64 / n as f64)).collect()
    }
    pub fn start_tangent(&self) -> P {
        let d = (self.b - self.a).norm();
        if self.is_line() {
            return d;
        }
        let half = self.sweep() / 2.0;
        rotate(d, -half)
    }
    pub fn end_tangent(&self) -> P {
        let d = (self.b - self.a).norm();
        if self.is_line() {
            return d;
        }
        let half = self.sweep() / 2.0;
        rotate(d, half)
    }
    pub fn reversed(&self) -> Seg {
        Seg { a: self.b, b: self.a, bulge: -self.bulge }
    }
    pub fn dist_to(&self, q: P) -> f64 {
        match self.arc() {
            None => dist_point_segment(q, self.a, self.b),
            Some(ai) => {
                let v = q - ai.c;
                let ang = v.angle();
                // fraction of sweep where q's angle lies
                let mut d = ang - ai.a0;
                if ai.sweep >= 0.0 {
                    d = d.rem_euclid(2.0 * PI);
                    if d <= ai.sweep {
                        return (v.len() - ai.r).abs();
                    }
                } else {
                    d = (-d).rem_euclid(2.0 * PI);
                    if d <= -ai.sweep {
                        return (v.len() - ai.r).abs();
                    }
                }
                q.dist(self.a).min(q.dist(self.b))
            }
        }
    }
}

pub fn rotate(v: P, ang: f64) -> P {
    let (s, c) = ang.sin_cos();
    p(v.x * c - v.y * s, v.x * s + v.y * c)
}

pub fn dist_to_segs<'a>(q: P, segs: impl IntoIterator<Item = &'a Seg>) -> f64 {
    segs.into_iter().map(|s| s.dist_to(q)).fold(f64::INFINITY, f64::min)
}

pub fn dist_to_polyline(q: P, pts: &[P]) -> f64 {
    if pts.len() == 1 {
        return q.dist(pts[0]);
    }
    pts.windows(2)
        .map(|w| dist_point_segment(q, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Circle through three points, if not collinear.
pub fn circumcircle(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-14 {
        return None;
    }
    let a2 = a.dot(a);
    let b2 = b.dot(b);
    let c2 = c.dot(c);
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    let center = p(ux, uy);
    Some((center, center.dist(a)))
}

#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub min: P,
    pub max: P,
}

impl BBox {
    pub fn empty() -> BBox {
        BBox { min: p(f64::INFINITY, f64::INFINITY), max: p(f64::NEG_INFINITY, f64::NEG_INFINITY) }
    }
    pub fn add(&mut self, q: P) {
        if !q.is_finite() {
            return;
        }
        self.min.x = self.min.x.min(q.x);
        self.min.y = self.min.y.min(q.y);
        self.max.x = self.max.x.max(q.x);
        self.max.y = self.max.y.max(q.y);
    }
    pub fn union(&mut self, o: &BBox) {
        if o.is_valid() {
            self.add(o.min);
            self.add(o.max);
        }
    }
    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y
    }
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    pub fn center(&self) -> P {
        self.min.lerp(self.max, 0.5)
    }
}

/// 2D affine transform: x' = m00 x + m01 y + tx ; y' = m10 x + m11 y + ty
#[derive(Clone, Copy, Debug)]
pub struct Affine {
    pub m: [f64; 4],
    pub t: P,
}

impl Affine {
    pub fn identity() -> Affine {
        Affine { m: [1.0, 0.0, 0.0, 1.0], t: p(0.0, 0.0) }
    }
    pub fn apply(&self, q: P) -> P {
        p(self.m[0] * q.x + self.m[1] * q.y + self.t.x, self.m[2] * q.x + self.m[3] * q.y + self.t.y)
    }
    pub fn apply_vec(&self, v: P) -> P {
        p(self.m[0] * v.x + self.m[1] * v.y, self.m[2] * v.x + self.m[3] * v.y)
    }
    pub fn det(&self) -> f64 {
        self.m[0] * self.m[3] - self.m[1] * self.m[2]
    }
    /// Then: apply `self` first, then `o`.
    pub fn then(&self, o: &Affine) -> Affine {
        let a = &o.m;
        let b = &self.m;
        Affine {
            m: [
                a[0] * b[0] + a[1] * b[2],
                a[0] * b[1] + a[1] * b[3],
                a[2] * b[0] + a[3] * b[2],
                a[2] * b[1] + a[3] * b[3],
            ],
            t: o.apply(self.t),
        }
    }
    /// Uniform scale (possibly mirrored) + rotation? Returns the scale factor if so.
    pub fn similarity_scale(&self) -> Option<f64> {
        let c0 = p(self.m[0], self.m[2]);
        let c1 = p(self.m[1], self.m[3]);
        let s0 = c0.len();
        let s1 = c1.len();
        if s0 == 0.0 {
            return None;
        }
        if ((s0 - s1) / s0).abs() < 1e-9 && (c0.dot(c1) / (s0 * s1)).abs() < 1e-9 {
            Some(s0)
        } else {
            None
        }
    }
    pub fn is_identity(&self) -> bool {
        self.m == [1.0, 0.0, 0.0, 1.0] && self.t == p(0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulge_quarter_arc_roundtrip() {
        let a = p(1.0, 0.0);
        let b = p(0.0, 1.0);
        let m = p(0.5f64.sqrt(), 0.5f64.sqrt());
        let bu = bulge_through(a, m, b);
        assert!((bu - (PI / 8.0).tan()).abs() < 1e-12);
        let s = Seg::new(a, b, bu);
        let ai = s.arc().unwrap();
        assert!(ai.c.dist(p(0.0, 0.0)) < 1e-12);
        assert!((ai.r - 1.0).abs() < 1e-12);
        assert!(s.midpoint().dist(m) < 1e-12);
        assert!(s.start_tangent().dist(p(0.0, 1.0)) < 1e-12);
        assert!(s.end_tangent().dist(p(-1.0, 0.0)) < 1e-12);
        assert!((bulge_from_start_tangent(a, b, p(0.0, 1.0)) - bu).abs() < 1e-12);
        assert!((bulge_from_end_tangent(a, b, p(-1.0, 0.0)) - bu).abs() < 1e-12);
        // clockwise version
        let cw = bulge_through(b, m, a);
        assert!((cw + bu).abs() < 1e-12);
    }

    #[test]
    fn dist_to_arc() {
        let s = Seg::new(p(1.0, 0.0), p(-1.0, 0.0), 1.0); // upper semicircle, CCW
        assert!((s.dist_to(p(0.0, 2.0)) - 1.0).abs() < 1e-12);
        assert!((s.dist_to(p(0.0, -2.0)) - 5f64.sqrt()).abs() < 1e-12);
    }
}
