//! The "Clean Up" engine: converts splines / tessellated polylines into
//! the fewest straight lines and true circular arcs within a tolerance,
//! joins touching pieces into continuous paths and removes duplicates.

use crate::dxf_read::{Drawing, Entity, Geom};
use crate::geom::*;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct Options {
    /// Maximum allowed deviation from the original geometry (drawing units).
    pub tolerance: f64,
    /// Endpoints closer than this are joined into one continuous path.
    pub join_tolerance: f64,
    pub remove_duplicates: bool,
    pub join_paths: bool,
    pub move_to_origin: bool,
    /// Put everything on this layer (None = keep source layers).
    pub single_layer: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            tolerance: 0.05,
            join_tolerance: 0.05,
            remove_duplicates: true,
            join_paths: true,
            move_to_origin: false,
            single_layer: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Path {
    pub segs: Vec<Seg>,
    pub closed: bool,
    pub layer: String,
}

impl Path {
    pub fn start(&self) -> P {
        self.segs[0].a
    }
    pub fn end(&self) -> P {
        self.segs.last().unwrap().b
    }
    pub fn reverse(&mut self) {
        self.segs.reverse();
        for s in &mut self.segs {
            *s = s.reversed();
        }
    }
    pub fn bbox(&self) -> BBox {
        let mut b = BBox::empty();
        for s in &self.segs {
            for q in s.samples(if s.is_line() { 1 } else { 16 }) {
                b.add(q);
            }
        }
        b
    }
    pub fn length(&self) -> f64 {
        self.segs.iter().map(|s| s.length()).sum()
    }
}

#[derive(Clone, Debug)]
pub enum Shape {
    Path(Path),
    Circle { c: P, r: f64, layer: String },
}

impl Shape {
    pub fn layer(&self) -> &str {
        match self {
            Shape::Path(p) => &p.layer,
            Shape::Circle { layer, .. } => layer,
        }
    }
    pub fn bbox(&self) -> BBox {
        match self {
            Shape::Path(p) => p.bbox(),
            Shape::Circle { c, r, .. } => BBox { min: *c - p(*r, *r), max: *c + p(*r, *r) },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub lines: usize,
    pub arcs: usize,
    pub circles: usize,
    pub closed_paths: usize,
    pub open_paths: usize,
    pub duplicates_removed: usize,
    /// Max measured deviation from the source geometry.
    pub max_deviation: f64,
}

#[derive(Clone, Debug, Default)]
pub struct CleanResult {
    pub shapes: Vec<Shape>,
    pub stats: Stats,
}

const SAMPLES: usize = 48;
const MAX_DEPTH: usize = 14;

// ---------------------------------------------------------------- fitting helpers

fn max_dev(segs: &[Seg], samples: &[P]) -> f64 {
    let fwd = samples.iter().map(|q| dist_to_segs(*q, segs)).fold(0.0, f64::max);
    let mut back: f64 = 0.0;
    for s in segs {
        for q in s.samples(8) {
            back = back.max(dist_to_polyline(q, samples));
        }
    }
    fwd.max(back)
}

/// Arc from a to b (endpoints fixed) whose bulge minimises the max deviation to `samples`.
fn best_arc(a: P, b: P, init_bulge: f64, samples: &[P], tol: f64) -> (f64, f64) {
    let dev = |bu: f64| {
        let s = Seg::with_support(a, b, bu, Vec::new());
        samples.iter().map(|q| s.dist_to(*q)).fold(0.0, f64::max)
    };
    let d0 = dev(init_bulge);
    if d0 <= tol * 0.5 {
        // the natural arc (through the curve's own points) is good enough: keep its exact radius
        return (init_bulge, d0);
    }
    let span = (init_bulge.abs() * 0.05).max(1e-6);
    let (mut lo, mut hi) = (init_bulge - span, init_bulge + span);
    let g = (5f64.sqrt() - 1.0) / 2.0;
    let mut x1 = hi - g * (hi - lo);
    let mut x2 = lo + g * (hi - lo);
    let (mut f1, mut f2) = (dev(x1), dev(x2));
    for _ in 0..40 {
        if f1 < f2 {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - g * (hi - lo);
            f1 = dev(x1);
        } else {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + g * (hi - lo);
            f2 = dev(x2);
        }
    }
    let (bx, bf) = if f1 < f2 { (x1, f1) } else { (x2, f2) };
    if bf < d0 { (bx, bf) } else { (init_bulge, d0) }
}

fn biarc(p1: P, t1: P, p2: P, t2: P) -> Option<(Seg, Seg)> {
    let v = p2 - p1;
    if v.len() < 1e-12 {
        return None;
    }
    let t = t1 + t2;
    let denom = 2.0 * (1.0 - t1.dot(t2));
    let d = if denom.abs() < 1e-12 {
        let vt2 = v.dot(t2);
        if vt2.abs() < 1e-12 {
            return None;
        }
        v.dot(v) / (4.0 * vt2)
    } else {
        let vt = v.dot(t);
        let disc = vt * vt + denom * v.dot(v);
        if disc < 0.0 {
            return None;
        }
        (-vt + disc.sqrt()) / denom
    };
    if !(d > 0.0) || !d.is_finite() {
        return None;
    }
    let pm = (p1 + t1 * d + p2 - t2 * d) * 0.5;
    if pm.dist(p1) < 1e-12 || pm.dist(p2) < 1e-12 {
        return None;
    }
    let s1 = Seg::with_support(p1, pm, bulge_from_start_tangent(p1, pm, t1), Vec::new());
    let s2 = Seg::with_support(pm, p2, bulge_from_end_tangent(pm, p2, t2), Vec::new());
    Some((s1, s2))
}

fn assign_support(segs: &mut [Seg], samples: &[P]) {
    for s in segs.iter_mut() {
        s.support.clear();
        s.support.push(s.a);
    }
    for q in samples {
        let (mut bi, mut bd) = (0, f64::INFINITY);
        for (i, s) in segs.iter().enumerate() {
            let d = s.dist_to(*q);
            if d < bd {
                bd = d;
                bi = i;
            }
        }
        segs[bi].support.push(*q);
    }
    for s in segs.iter_mut() {
        s.support.push(s.b);
    }
}

/// Fit a smooth parametric curve on [t0, t1] with lines/arcs/biarcs.
fn fit_parametric(f: &dyn Fn(f64) -> P, t0: f64, t1: f64, tol: f64, depth: usize, out: &mut Vec<Seg>) {
    let a = f(t0);
    let b = f(t1);
    let samples: Vec<P> = (0..=SAMPLES).map(|i| f(t0 + (t1 - t0) * i as f64 / SAMPLES as f64)).collect();
    if samples.iter().all(|q| q.dist(a) <= tol * 1e-3) {
        return; // degenerate (zero length)
    }
    let split = |out: &mut Vec<Seg>| {
        let tm = 0.5 * (t0 + t1);
        fit_parametric(f, t0, tm, tol, depth + 1, out);
        fit_parametric(f, tm, t1, tol, depth + 1, out);
    };
    if a.dist(b) <= tol {
        // closed or nearly closed piece: always split
        if depth < MAX_DEPTH {
            split(out);
            return;
        }
    }
    // 1. straight line
    if samples.iter().all(|q| dist_point_segment(*q, a, b) <= tol) {
        out.push(Seg::with_support(a, b, 0.0, samples));
        return;
    }
    // 2. single arc
    let m = f(0.5 * (t0 + t1));
    let bu0 = bulge_through(a, m, b);
    if bu0.is_finite() {
        let (bu, _) = best_arc(a, b, bu0, &samples, tol);
        let s = Seg::with_support(a, b, bu, Vec::new());
        if s.sweep().abs() < 2.0 * PI - 1e-3 && max_dev(std::slice::from_ref(&s), &samples) <= tol {
            out.push(Seg::with_support(a, b, bu, samples));
            return;
        }
    }
    // 3. biarc using end tangents
    let h = (t1 - t0) * 1e-5;
    let ta = (f(t0 + h) - a).norm();
    let tb = (b - f(t1 - h)).norm();
    if ta.len() > 0.5 && tb.len() > 0.5 {
        if let Some((s1, s2)) = biarc(a, ta, b, tb) {
            let mut segs = [s1, s2];
            if max_dev(&segs, &samples) <= tol {
                assign_support(&mut segs, &samples);
                out.extend(segs);
                return;
            }
        }
    }
    // 4. subdivide
    if depth < MAX_DEPTH {
        split(out);
    } else {
        for w in samples.windows(2) {
            if w[0].dist(w[1]) > 0.0 {
                out.push(Seg::with_support(w[0], w[1], 0.0, vec![w[0], w[1]]));
            }
        }
    }
}

fn line_ok(q: &[P], i: usize, j: usize, tol: f64) -> bool {
    let a = q[i];
    let b = q[j];
    let d = b - a;
    let l = d.len();
    if l <= 0.0 {
        return false;
    }
    let u = d * (1.0 / l);
    let mut last = 0.0;
    for k in i + 1..j {
        if dist_point_segment(q[k], a, b) > tol {
            return false;
        }
        let s = (q[k] - a).dot(u);
        if s < last - tol {
            return false; // backtracking
        }
        last = last.max(s);
    }
    true
}

fn arc_ok(q: &[P], i: usize, j: usize, tol: f64) -> Option<f64> {
    let m = (i + j) / 2;
    let bu = bulge_through(q[i], q[m], q[j]);
    if !bu.is_finite() {
        return None;
    }
    let s = Seg::with_support(q[i], q[j], bu, Vec::new());
    let ai = s.arc()?;
    if ai.sweep.abs() > 2.0 * PI - 1e-3 {
        return None;
    }
    let mut last = -1e-9;
    for k in i..=j {
        if k > i && k < j && s.dist_to(q[k]) > tol {
            return None;
        }
        if k < j && s.dist_to(q[k].lerp(q[k + 1], 0.5)) > tol {
            return None;
        }
        // monotone progress along the arc
        let ang = (q[k] - ai.c).angle() - ai.a0;
        let frac = if ai.sweep >= 0.0 { ang.rem_euclid(2.0 * PI) } else { (-ang).rem_euclid(2.0 * PI) } / ai.sweep.abs();
        let frac = if k == j { 1.0 } else if frac > 1.0 + 1e-6 && k == i { 0.0 } else { frac };
        if frac + 1e-6 < last || frac > 1.0 + 1e-6 {
            return None;
        }
        last = frac;
    }
    Some(bu)
}

/// Fit a run of points (tessellated outline) with the fewest lines and arcs.
pub fn fit_points(pts: &[P], tol: f64) -> Vec<Seg> {
    let mut q: Vec<P> = Vec::with_capacity(pts.len());
    for &x in pts {
        if q.last().map(|l: &P| l.dist(x) > tol * 1e-3).unwrap_or(true) {
            q.push(x);
        }
    }
    let mut out = Vec::new();
    if q.len() < 2 {
        return out;
    }
    let n = q.len() - 1;
    let mut i = 0;
    while i < n {
        let mut jl = i + 1;
        while jl < n && line_ok(&q, i, jl + 1, tol) {
            jl += 1;
        }
        let mut ja: Option<(usize, f64)> = None;
        let mut j = i + 3;
        while j <= n && j - i <= 5000 {
            match arc_ok(&q, i, j, tol) {
                Some(bu) => {
                    ja = Some((j, bu));
                    j += 1;
                }
                None => break,
            }
        }
        match ja {
            Some((j, bu)) if j > jl => {
                let sup = q[i..=j].to_vec();
                let (bu, _) = best_arc(q[i], q[j], bu, &sup, tol);
                out.push(Seg::with_support(q[i], q[j], bu, sup));
                i = j;
            }
            _ => {
                out.push(Seg::with_support(q[i], q[jl], 0.0, q[i..=jl].to_vec()));
                i = jl;
            }
        }
    }
    out
}

// ---------------------------------------------------------------- NURBS

struct Nurbs<'a> {
    p: usize,
    k: &'a [f64],
    c: &'a [P],
    w: &'a [f64],
}

impl Nurbs<'_> {
    fn eval_span(&self, span: usize, t: f64) -> P {
        let p = self.p;
        let mut d: Vec<[f64; 3]> = (0..=p)
            .map(|j| {
                let i = j + span - p;
                let w = self.w[i];
                [self.c[i].x * w, self.c[i].y * w, w]
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i0 = j + span - p;
                let den = self.k[j + 1 + span - r] - self.k[i0];
                let alpha = if den.abs() < 1e-300 { 0.0 } else { (t - self.k[i0]) / den };
                for c in 0..3 {
                    d[j][c] = (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c];
                }
            }
        }
        let h = d[p];
        if h[2].abs() < 1e-300 { crate::geom::p(h[0], h[1]) } else { crate::geom::p(h[0] / h[2], h[1] / h[2]) }
    }
    fn spans(&self) -> Vec<(usize, f64, f64)> {
        let n = self.c.len();
        (self.p..n).filter(|&s| self.k[s + 1] > self.k[s]).map(|s| (s, self.k[s], self.k[s + 1])).collect()
    }
}

// ---------------------------------------------------------------- entity -> segments

fn arc_segs(c: P, r: f64, a0: f64, a1: f64) -> Vec<Seg> {
    let sweep = a1 - a0;
    let n = (sweep.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let pt = |a: f64| c + p(a.cos(), a.sin()) * r;
    (0..n)
        .map(|i| {
            let s0 = a0 + sweep * i as f64 / n as f64;
            let s1 = a0 + sweep * (i + 1) as f64 / n as f64;
            Seg::new(pt(s0), pt(s1), ((s1 - s0) / 4.0).tan())
        })
        .collect()
}

enum Converted {
    Segs(Vec<Seg>, bool),
    Circle(P, f64),
    None,
}

fn convert_entity(e: &Entity, tol: f64) -> Converted {
    match &e.geom {
        Geom::Line { a, b } => {
            if a.dist(*b) <= tol * 1e-3 {
                Converted::None
            } else {
                Converted::Segs(vec![Seg::new(*a, *b, 0.0)], false)
            }
        }
        Geom::Circle { c, r } => Converted::Circle(*c, *r),
        Geom::Arc { c, r, a0, a1 } => {
            if a1 - a0 >= 2.0 * PI - 1e-9 {
                Converted::Circle(*c, *r)
            } else {
                Converted::Segs(arc_segs(*c, *r, *a0, *a1), false)
            }
        }
        Geom::Ellipse { c, u, v, t0, t1 } => {
            let full = t1 - t0 >= 2.0 * PI - 1e-9;
            let circ = ((u.len() - v.len()).abs() <= 1e-9 * u.len()) && u.dot(*v).abs() <= 1e-9 * u.len() * v.len();
            if full && circ {
                return Converted::Circle(*c, u.len());
            }
            let f = |t: f64| *c + *u * t.cos() + *v * t.sin();
            let n = ((t1 - t0) / (PI / 2.0)).ceil().max(1.0) as usize;
            let mut segs = Vec::new();
            for i in 0..n {
                let s0 = t0 + (t1 - t0) * i as f64 / n as f64;
                let s1 = t0 + (t1 - t0) * (i + 1) as f64 / n as f64;
                fit_parametric(&f, s0, s1, tol, 0, &mut segs);
            }
            Converted::Segs(segs, full)
        }
        Geom::Poly { pts, bulges, closed } => {
            let n = pts.len();
            let nseg = if *closed { n } else { n - 1 };
            let mut segs = Vec::new();
            let mut run: Vec<P> = Vec::new();
            let flush = |run: &mut Vec<P>, segs: &mut Vec<Seg>| {
                if run.len() >= 2 {
                    segs.extend(fit_points(run, tol));
                }
                run.clear();
            };
            for k in 0..nseg {
                let a = pts[k];
                let b = pts[(k + 1) % n];
                let bu = bulges.get(k).copied().unwrap_or(0.0);
                if bu.abs() < LINE_BULGE_EPS {
                    if run.is_empty() {
                        run.push(a);
                    }
                    run.push(b);
                } else {
                    flush(&mut run, &mut segs);
                    if a.dist(b) > tol * 1e-3 {
                        let s = Seg::new(a, b, bu);
                        // split big arcs into <= 90 degree pieces for robust merging
                        if let Some(ai) = s.arc() {
                            if ai.sweep.abs() > PI / 2.0 + 1e-9 {
                                segs.extend(arc_segs(ai.c, ai.r, ai.a0, ai.a0 + ai.sweep));
                                continue;
                            }
                        }
                        segs.push(s);
                    }
                }
            }
            flush(&mut run, &mut segs);
            Converted::Segs(segs, *closed)
        }
        Geom::Spline { degree, knots, ctrl, weights, fit, closed } => {
            let mut segs = Vec::new();
            if !ctrl.is_empty() {
                let nb = Nurbs { p: *degree, k: knots, c: ctrl, w: weights };
                for (span, ka, kb) in nb.spans() {
                    let f = |t: f64| nb.eval_span(span, t);
                    fit_parametric(&f, ka, kb, tol, 0, &mut segs);
                }
            } else {
                segs = fit_points(fit, tol);
            }
            Converted::Segs(segs, *closed)
        }
    }
}

// ---------------------------------------------------------------- merging

const MAX_SUPPORT: usize = 400;

fn merged_support(a: &Seg, b: &Seg) -> Vec<P> {
    let mut v = Vec::with_capacity(a.support.len() + b.support.len());
    v.extend_from_slice(&a.support);
    v.extend_from_slice(&b.support);
    while v.len() > MAX_SUPPORT {
        // keep endpoints, decimate the rest
        let last = *v.last().unwrap();
        v = v.iter().step_by(2).copied().collect();
        if v.last() != Some(&last) {
            v.push(last);
        }
    }
    v
}

fn try_merge(s1: &Seg, s2: &Seg, tol: f64) -> Option<Seg> {
    let l1 = s1.is_line();
    let l2 = s2.is_line();
    let sup = merged_support(s1, s2);
    let cand = if l1 && l2 {
        if (s1.b - s1.a).dot(s2.b - s2.a) <= 0.0 {
            return None;
        }
        if s1.a.dist(s2.b) <= tol {
            return None;
        }
        Seg::with_support(s1.a, s2.b, 0.0, Vec::new())
    } else if !l1 && !l2 {
        if s1.bulge.signum() != s2.bulge.signum() {
            return None;
        }
        let total = s1.sweep() + s2.sweep();
        if total.abs() >= 2.0 * PI - 1e-3 {
            return None;
        }
        let (a1, a2) = (s1.arc()?, s2.arc()?);
        if (a1.r - a2.r).abs() > 2.0 * tol || a1.c.dist(a2.c) > 2.0 * tol {
            return None;
        }
        let bu0 = (total / 4.0).tan();
        let (bu, _) = best_arc(s1.a, s2.b, bu0, &sup, tol);
        Seg::with_support(s1.a, s2.b, bu, Vec::new())
    } else {
        // one arc + one (short) line: absorb the line into the arc if it lies on the same circle
        let arc_seg = if l1 { s2 } else { s1 };
        let bu0 = bulge_through(s1.a, s1.b, s2.b);
        if !bu0.is_finite() || bu0.signum() != arc_seg.bulge.signum() {
            return None;
        }
        let total = 4.0 * bu0.atan();
        if total.abs() >= 2.0 * PI - 1e-3 {
            return None;
        }
        let (bu, _) = best_arc(s1.a, s2.b, bu0, &sup, tol);
        Seg::with_support(s1.a, s2.b, bu, Vec::new())
    };
    // validate both ways
    if sup.iter().any(|q| cand.dist_to(*q) > tol) {
        return None;
    }
    let pair = [s1.clone(), s2.clone()];
    if cand.samples(12).iter().any(|q| dist_to_segs(*q, &pair) > tol) {
        return None;
    }
    Some(Seg { support: sup, ..cand })
}

fn simplify(path: &mut Path, tol: f64) {
    let tiny = tol * 1e-3;
    let mut out: Vec<Seg> = Vec::with_capacity(path.segs.len());
    for s in path.segs.drain(..) {
        if s.chord() <= tiny && s.is_line() {
            // drop, but keep continuity
            if let Some(last) = out.last_mut() {
                last.b = s.b;
            }
            continue;
        }
        if let Some(last) = out.last() {
            if let Some(m) = try_merge(last, &s, tol) {
                *out.last_mut().unwrap() = m;
                continue;
            }
        }
        out.push(s);
    }
    if path.closed {
        while out.len() >= 2 {
            let last = out.last().unwrap();
            match try_merge(last, &out[0], tol) {
                Some(m) => {
                    out.pop();
                    out[0] = m;
                }
                None => break,
            }
        }
    }
    path.segs = out;
}

fn as_circle(path: &Path, tol: f64) -> Option<(P, f64)> {
    if !path.closed || path.segs.is_empty() {
        return None;
    }
    let infos: Vec<ArcInfo> = path.segs.iter().filter_map(|s| s.arc()).collect();
    if infos.is_empty() {
        return None;
    }
    // weight by arc length so tiny pieces do not skew the result
    let wsum: f64 = infos.iter().map(|ai| ai.r * ai.sweep.abs()).sum();
    if wsum <= 0.0 {
        return None;
    }
    let c = infos.iter().fold(p(0.0, 0.0), |acc, ai| acc + ai.c * (ai.r * ai.sweep.abs())) * (1.0 / wsum);
    // best-fit radius around that center (min-max of all original samples)
    let (mut dmin, mut dmax) = (f64::INFINITY, 0.0f64);
    for s in &path.segs {
        for q in s.support.iter().chain([&s.a, &s.b]) {
            let d = q.dist(c);
            dmin = dmin.min(d);
            dmax = dmax.max(d);
        }
    }
    if (dmax - dmin) / 2.0 > tol {
        return None;
    }
    // prefer the arcs' own (designed) radius; fall back to the min-max radius
    let r_avg = infos.iter().map(|ai| ai.r * ai.sweep.abs()).sum::<f64>() / infos.iter().map(|ai| ai.sweep.abs()).sum::<f64>();
    let r = if (dmax - r_avg).abs().max((r_avg - dmin).abs()) <= tol * 0.5 { r_avg } else { (dmax + dmin) / 2.0 };
    let mut total = 0.0;
    for s in &path.segs {
        if s.is_line() {
            let half = s.chord() / 2.0;
            if half > r || r - (r * r - half * half).sqrt() > tol {
                return None;
            }
            total += signed_angle(s.a - c, s.b - c);
        } else {
            total += s.sweep();
        }
        // verify against the original samples
        for q in s.support.iter().chain([&s.a, &s.b]) {
            if (q.dist(c) - r).abs() > tol {
                return None;
            }
        }
    }
    if (total.abs() - 2.0 * PI).abs() > 1e-2 {
        return None;
    }
    Some((c, r))
}

// ---------------------------------------------------------------- joining

fn join_paths(paths: Vec<Path>, jt: f64) -> Vec<Path> {
    let n = paths.len();
    let mut used = vec![false; n];
    let mut out = Vec::new();
    for i in 0..n {
        if used[i] {
            continue;
        }
        used[i] = true;
        let mut cur = paths[i].clone();
        if !cur.closed {
            // extend forward
            loop {
                let end = cur.end();
                let mut best: Option<(usize, bool, f64)> = None;
                for j in 0..n {
                    if used[j] || paths[j].closed || paths[j].layer != cur.layer {
                        continue;
                    }
                    let ds = paths[j].start().dist(end);
                    let de = paths[j].end().dist(end);
                    for (d, rev) in [(ds, false), (de, true)] {
                        if d <= jt && best.map(|b| d < b.2).unwrap_or(true) {
                            best = Some((j, rev, d));
                        }
                    }
                }
                let Some((j, rev, _)) = best else { break };
                used[j] = true;
                let mut nxt = paths[j].clone();
                if rev {
                    nxt.reverse();
                }
                nxt.segs[0].a = end;
                cur.segs.extend(nxt.segs);
            }
            // extend backward
            loop {
                let start = cur.start();
                let mut best: Option<(usize, bool, f64)> = None;
                for j in 0..n {
                    if used[j] || paths[j].closed || paths[j].layer != cur.layer {
                        continue;
                    }
                    let de = paths[j].end().dist(start);
                    let ds = paths[j].start().dist(start);
                    for (d, rev) in [(de, false), (ds, true)] {
                        if d <= jt && best.map(|b| d < b.2).unwrap_or(true) {
                            best = Some((j, rev, d));
                        }
                    }
                }
                let Some((j, rev, _)) = best else { break };
                used[j] = true;
                let mut prv = paths[j].clone();
                if rev {
                    prv.reverse();
                }
                prv.segs.last_mut().unwrap().b = start;
                prv.segs.extend(cur.segs);
                cur.segs = prv.segs;
            }
        }
        close_if_touching(&mut cur, jt);
        out.push(cur);
    }
    out
}

fn close_if_touching(path: &mut Path, jt: f64) {
    if path.closed || path.segs.is_empty() {
        return;
    }
    let len = path.length();
    if path.start().dist(path.end()) <= jt && len > 4.0 * jt && (path.segs.len() >= 2 || !path.segs[0].is_line()) {
        let s = path.start();
        path.segs.last_mut().unwrap().b = s;
        path.closed = true;
    }
}

// ---------------------------------------------------------------- duplicates

fn shapes_equal(a: &Shape, b: &Shape, tol: f64) -> bool {
    match (a, b) {
        (Shape::Circle { c: c1, r: r1, .. }, Shape::Circle { c: c2, r: r2, .. }) => {
            c1.dist(*c2) <= tol && (r1 - r2).abs() <= tol
        }
        (Shape::Path(p1), Shape::Path(p2)) => {
            if p1.closed != p2.closed {
                return false;
            }
            let (b1, b2) = (p1.bbox(), p2.bbox());
            if b1.min.dist(b2.min) > tol || b1.max.dist(b2.max) > tol {
                return false;
            }
            if (p1.length() - p2.length()).abs() > tol * (p1.segs.len() + p2.segs.len()) as f64 {
                return false;
            }
            let covers = |x: &Path, y: &Path| {
                x.segs.iter().all(|s| s.samples(if s.is_line() { 2 } else { 6 }).iter().all(|q| dist_to_segs(*q, &y.segs) <= tol))
            };
            covers(p1, p2) && covers(p2, p1)
        }
        _ => false,
    }
}

fn dedupe(shapes: Vec<Shape>, tol: f64, removed: &mut usize) -> Vec<Shape> {
    let mut keep: Vec<Shape> = Vec::with_capacity(shapes.len());
    let mut boxes: Vec<BBox> = Vec::new();
    for s in shapes {
        let bb = s.bbox();
        let dup = keep.iter().zip(&boxes).any(|(k, kb)| {
            kb.min.dist(bb.min) <= tol && kb.max.dist(bb.max) <= tol && shapes_equal(k, &s, tol)
        });
        if dup {
            *removed += 1;
        } else {
            boxes.push(bb);
            keep.push(s);
        }
    }
    keep
}

// ---------------------------------------------------------------- public API

pub fn clean(dwg: &Drawing, opt: &Options) -> CleanResult {
    let tol = opt.tolerance.max(1e-6);
    let jt = opt.join_tolerance.max(0.0);
    let mut stats = Stats::default();
    let layer_of = |e: &Entity| opt.single_layer.clone().unwrap_or_else(|| e.layer.clone());

    // 1. convert every entity
    let mut shapes: Vec<Shape> = Vec::new();
    for e in &dwg.entities {
        match convert_entity(e, tol) {
            Converted::Circle(c, r) => shapes.push(Shape::Circle { c, r, layer: layer_of(e) }),
            Converted::Segs(segs, closed) => {
                if segs.is_empty() {
                    continue;
                }
                let mut path = Path { segs, closed, layer: layer_of(e) };
                if path.closed {
                    let s = path.start();
                    if path.end().dist(s) > tiny_gap(tol) {
                        // closing segment for closed polylines that do not repeat the first vertex
                        let e2 = path.end();
                        path.segs.push(Seg::new(e2, s, 0.0));
                    } else {
                        path.segs.last_mut().unwrap().b = s;
                    }
                } else {
                    close_if_touching(&mut path, jt.max(tol * 1e-3));
                }
                shapes.push(Shape::Path(path));
            }
            Converted::None => {}
        }
    }

    // 2. drop duplicates (before joining, so a path never joins its own copy)
    if opt.remove_duplicates {
        shapes = dedupe(shapes, tol, &mut stats.duplicates_removed);
    }

    // 3. join touching open paths
    let mut circles = Vec::new();
    let mut paths = Vec::new();
    for s in shapes {
        match s {
            Shape::Path(p) => paths.push(p),
            c => circles.push(c),
        }
    }
    if opt.join_paths && jt > 0.0 {
        paths = join_paths(paths, jt);
    }

    // 4. merge collinear lines / co-circular arcs, detect circles
    let mut shapes: Vec<Shape> = circles;
    for mut path in paths {
        simplify(&mut path, tol);
        if path.segs.is_empty() {
            continue;
        }
        if let Some((c, r)) = as_circle(&path, tol) {
            shapes.push(Shape::Circle { c, r, layer: path.layer.clone() });
        } else {
            shapes.push(Shape::Path(path));
        }
    }
    if opt.remove_duplicates {
        shapes = dedupe(shapes, tol, &mut stats.duplicates_removed);
    }

    // 5. deviation report
    for s in &shapes {
        if let Shape::Path(path) = s {
            for seg in &path.segs {
                for q in &seg.support {
                    stats.max_deviation = stats.max_deviation.max(seg.dist_to(*q));
                }
            }
        }
    }

    // 6. optional move to origin
    if opt.move_to_origin {
        let mut bb = BBox::empty();
        for s in &shapes {
            bb.union(&s.bbox());
        }
        if bb.is_valid() {
            let d = bb.min;
            for s in &mut shapes {
                match s {
                    Shape::Circle { c, .. } => *c = *c - d,
                    Shape::Path(path) => {
                        for seg in &mut path.segs {
                            seg.a = seg.a - d;
                            seg.b = seg.b - d;
                            for q in &mut seg.support {
                                *q = *q - d;
                            }
                        }
                    }
                }
            }
        }
    }

    for s in &shapes {
        match s {
            Shape::Circle { .. } => stats.circles += 1,
            Shape::Path(p) => {
                if p.closed {
                    stats.closed_paths += 1;
                } else {
                    stats.open_paths += 1;
                }
                for seg in &p.segs {
                    if seg.is_line() {
                        stats.lines += 1;
                    } else {
                        stats.arcs += 1;
                    }
                }
            }
        }
    }
    CleanResult { shapes, stats }
}

fn tiny_gap(tol: f64) -> f64 {
    tol * 1e-3
}

/// Sampled outline of a source entity, for the "before" preview.
/// Returns (polyline, nodes) where nodes are the points a cutter would treat as vertices.
pub fn entity_outline(e: &Entity) -> (Vec<P>, Vec<P>) {
    match &e.geom {
        Geom::Line { a, b } => (vec![*a, *b], vec![*a, *b]),
        Geom::Circle { c, r } => {
            let pts: Vec<P> = (0..=96).map(|i| *c + p((i as f64 / 96.0 * 2.0 * PI).cos(), (i as f64 / 96.0 * 2.0 * PI).sin()) * *r).collect();
            (pts, vec![])
        }
        Geom::Arc { c, r, a0, a1 } => {
            let n = 64;
            let pts: Vec<P> = (0..=n).map(|i| {
                let a = a0 + (a1 - a0) * i as f64 / n as f64;
                *c + p(a.cos(), a.sin()) * *r
            }).collect();
            let nodes = vec![pts[0], pts[n]];
            (pts, nodes)
        }
        Geom::Ellipse { c, u, v, t0, t1 } => {
            let n = 96;
            let pts: Vec<P> = (0..=n).map(|i| {
                let t = t0 + (t1 - t0) * i as f64 / n as f64;
                *c + *u * t.cos() + *v * t.sin()
            }).collect();
            (pts, vec![])
        }
        Geom::Poly { pts, bulges, closed } => {
            let n = pts.len();
            let nseg = if *closed { n } else { n - 1 };
            let mut out = vec![pts[0]];
            for k in 0..nseg {
                let s = Seg::with_support(pts[k], pts[(k + 1) % n], bulges.get(k).copied().unwrap_or(0.0), vec![]);
                let smp = s.samples(if s.is_line() { 1 } else { 24 });
                out.extend_from_slice(&smp[1..]);
            }
            (out, pts.clone())
        }
        Geom::Spline { degree, knots, ctrl, weights, fit, .. } => {
            if ctrl.is_empty() {
                return (fit.clone(), fit.clone());
            }
            let nb = Nurbs { p: *degree, k: knots, c: ctrl, w: weights };
            let mut out = Vec::new();
            let mut nodes = Vec::new();
            for (span, ka, kb) in nb.spans() {
                for i in 0..=24 {
                    out.push(nb.eval_span(span, ka + (kb - ka) * i as f64 / 24.0));
                }
                nodes.push(nb.eval_span(span, ka));
            }
            nodes.extend_from_slice(ctrl);
            (out, nodes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf_read::{Drawing, Entity, Geom};

    fn ent(geom: Geom) -> Entity {
        Entity { layer: "0".into(), kind: "TEST", geom }
    }

    #[test]
    fn tessellated_circle_becomes_circle() {
        let pts: Vec<P> = (0..360).map(|i| {
            let a = i as f64 / 360.0 * 2.0 * PI;
            p(50.0 + 20.0 * a.cos(), 10.0 + 20.0 * a.sin())
        }).collect();
        let n = pts.len();
        let d = Drawing { entities: vec![ent(Geom::Poly { pts, bulges: vec![0.0; n], closed: true })], ..Default::default() };
        let r = clean(&d, &Options::default());
        assert_eq!(r.shapes.len(), 1);
        match &r.shapes[0] {
            Shape::Circle { c, r, .. } => {
                assert!(c.dist(p(50.0, 10.0)) < 0.01);
                assert!((r - 20.0).abs() < 0.01);
            }
            other => panic!("expected circle, got {other:?}"),
        }
    }

    #[test]
    fn rounded_rectangle_from_points() {
        // rectangle 100x50 with 10mm corner radii, densely tessellated
        let mut pts = Vec::new();
        let corners = [(90.0, 40.0, 0.0), (10.0, 40.0, 90.0), (10.0, 10.0, 180.0), (90.0, 10.0, 270.0)];
        for (cx, cy, a0) in corners {
            for i in 0..=20 {
                let a = (a0 + 90.0 * i as f64 / 20.0f64).to_radians();
                pts.push(p(cx + 10.0 * a.cos(), cy + 10.0 * a.sin()));
            }
            // add some redundant points along the following straight edge
        }
        let n = pts.len();
        let d = Drawing { entities: vec![ent(Geom::Poly { pts, bulges: vec![0.0; n], closed: true })], ..Default::default() };
        let r = clean(&d, &Options::default());
        assert_eq!(r.shapes.len(), 1);
        assert_eq!(r.stats.arcs, 4, "{:?}", r.stats);
        assert_eq!(r.stats.lines, 4, "{:?}", r.stats);
        assert_eq!(r.stats.closed_paths, 1);
    }

    #[test]
    fn bezier_circle_becomes_circle() {
        // Illustrator-style circle: 4 cubic Beziers, kappa = 0.5523
        let k = 0.552_284_749_8;
        let r = 30.0;
        let c = p(100.0, -50.0);
        let pts = [
            p(r, 0.0), p(r, k * r), p(k * r, r), p(0.0, r), p(-k * r, r), p(-r, k * r), p(-r, 0.0),
            p(-r, -k * r), p(-k * r, -r), p(0.0, -r), p(k * r, -r), p(r, -k * r), p(r, 0.0),
        ];
        let ctrl: Vec<P> = pts.iter().map(|q| *q + c).collect();
        let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0, 4.0];
        let d = Drawing {
            entities: vec![ent(Geom::Spline { degree: 3, knots, weights: vec![1.0; 13], ctrl, fit: vec![], closed: true })],
            ..Default::default()
        };
        let res = clean(&d, &Options::default());
        assert_eq!(res.shapes.len(), 1, "{:?}", res.stats);
        assert!(matches!(res.shapes[0], Shape::Circle { .. }), "{:?}", res.stats);
    }

    fn bezier_spline(ctrl: Vec<P>) -> Entity {
        let nseg = (ctrl.len() - 1) / 3;
        let mut knots = vec![0.0; 4];
        for i in 1..nseg {
            knots.extend([i as f64; 3]);
        }
        knots.extend([nseg as f64; 4]);
        let n = ctrl.len();
        ent(Geom::Spline { degree: 3, knots, weights: vec![1.0; n], ctrl, fit: vec![], closed: false })
    }

    #[test]
    fn large_bezier_circle_and_freeform_within_tolerance() {
        let k = 0.552_284_749_8;
        let r = 250.0;
        let pts = [
            p(r, 0.0), p(r, k * r), p(k * r, r), p(0.0, r), p(-k * r, r), p(-r, k * r), p(-r, 0.0),
            p(-r, -k * r), p(-k * r, -r), p(0.0, -r), p(k * r, -r), p(r, -k * r), p(r, 0.0),
        ];
        let circle = bezier_spline(pts.to_vec());
        let s_curve = bezier_spline(vec![p(0.0, 0.0), p(100.0, 0.0), p(0.0, 100.0), p(100.0, 100.0)]);
        let d = Drawing { entities: vec![circle, s_curve], ..Default::default() };
        let opt = Options::default();
        let res = clean(&d, &opt);
        assert_eq!(res.stats.circles, 1, "{:?}", res.stats);
        assert_eq!(res.stats.open_paths, 1, "{:?}", res.stats);
        assert!(res.stats.max_deviation <= opt.tolerance + 1e-9, "{:?}", res.stats);
        // free-form curve: arcs only, tangent-continuous
        let path = res.shapes.iter().find_map(|s| if let Shape::Path(p) = s { Some(p) } else { None }).unwrap();
        assert!(path.segs.len() < 40, "too many segments: {}", path.segs.len());
        for w in path.segs.windows(2) {
            let turn = signed_angle(w[0].end_tangent(), w[1].start_tangent()).abs();
            assert!(turn < 0.05, "kink of {turn} rad");
        }
    }

    #[test]
    fn split_lines_join_into_closed_square() {
        let sq = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
        let mut ents = Vec::new();
        for i in 0..4 {
            // each edge split into 3 collinear pieces, some reversed
            let a = sq[i];
            let b = sq[(i + 1) % 4];
            for k in 0..3 {
                let s = a.lerp(b, k as f64 / 3.0);
                let e = a.lerp(b, (k + 1) as f64 / 3.0);
                let g = if k == 1 { Geom::Line { a: e, b: s } } else { Geom::Line { a: s, b: e } };
                ents.push(ent(g));
            }
        }
        ents.push(ents[0].clone()); // duplicate
        let d = Drawing { entities: ents, ..Default::default() };
        let r = clean(&d, &Options::default());
        assert_eq!(r.stats.closed_paths, 1, "{:?}", r.stats);
        assert_eq!(r.stats.lines, 4, "{:?}", r.stats);
        assert_eq!(r.stats.duplicates_removed, 1);
    }
}
