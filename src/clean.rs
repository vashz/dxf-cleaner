//! The "Clean Up" engine: converts splines / tessellated polylines into
//! the fewest straight lines and true circular arcs within a tolerance,
//! joins touching pieces into continuous paths and removes duplicates.

use crate::dxf_read::{Drawing, Entity};
use crate::geom::*;
use crate::spans::Span;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct Options {
    /// Maximum allowed deviation from the original geometry (drawing units).
    pub tolerance: f64,
    /// Endpoints closer than this are joined into one continuous path.
    pub join_gap: f64,
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
            join_gap: 0.05,
            remove_duplicates: true,
            join_paths: true,
            move_to_origin: false,
            single_layer: None,
        }
    }
}

impl Options {
    /// The window's "Put everything on layer 0" and `dxf-clean --layer0`: on puts every
    /// shape on layer 0, off keeps the source layers.
    pub fn put_everything_on_layer_0(&mut self, on: bool) {
        self.single_layer = on.then(|| "0".to_string());
    }
}

/// The two distances every closing and joining decision needs.
#[derive(Clone, Copy, Debug)]
struct Limits {
    /// How far original geometry may move (see `Options::tolerance`).
    tol: f64,
    /// How far apart two path ends may be and still be joined (see `Options::join_gap`).
    join_gap: f64,
}

/// A continuous run of segments on one layer: each segment starts exactly where the previous
/// one ends, and a closed path ends exactly where it starts.
///
/// Only `Path`'s own methods move segment endpoints (closing, joining, dropping negligible
/// segments), so no join can move original geometry past the tolerance; see [`meet`].
#[derive(Clone, Debug)]
pub struct Path {
    segs: Vec<Fitted>,
    closed: bool,
    layer: String,
}

/// One segment of a path, with the points on the original geometry it replaces. Every merge,
/// meet and circle check is verified against those points, so no change to a path can move the
/// original geometry past the tolerance unnoticed.
#[derive(Clone, Debug)]
struct Fitted {
    seg: Seg,
    /// Points on the original geometry this segment replaces, in order from `seg.a` to `seg.b`.
    original: Vec<P>,
}

/// Merged segments keep at most this many original samples.
const MAX_ORIGINAL: usize = 400;

impl Fitted {
    fn new(seg: Seg, original: Vec<P>) -> Fitted {
        Fitted { seg, original }
    }

    /// A segment that is itself original geometry (a line, an arc, a polyline span), sampled
    /// from its own points.
    fn exact(seg: Seg) -> Fitted {
        let original = seg.samples(if seg.is_line() { 2 } else { 8 });
        Fitted { seg, original }
    }

    fn reversed(&self) -> Fitted {
        let mut original = self.original.clone();
        original.reverse();
        Fitted { seg: self.seg.reversed(), original }
    }

    /// Moves the segment and its original samples by `d`.
    fn translate(&mut self, d: P) {
        self.seg.a = self.seg.a + d;
        self.seg.b = self.seg.b + d;
        for q in &mut self.original {
            *q = *q + d;
        }
    }

    /// Whether every original sample lies within `tol` of the segment. A sample whose distance
    /// cannot be measured (NaN) fails.
    fn traces_original(&self, tol: f64) -> bool {
        self.original.iter().all(|q| self.seg.dist_to(*q) <= tol)
    }

    /// The segment split at its midpoint, each half keeping the original samples nearest to it.
    fn halves(&self) -> (Fitted, Fitted) {
        let s = &self.seg;
        let m = s.midpoint();
        let bulge = (s.bulge.atan() / 2.0).tan();
        let (h1, h2) = (Seg::new(s.a, m, bulge), Seg::new(m, s.b, bulge));
        let (o1, o2): (Vec<P>, Vec<P>) = self.original.iter().partition(|q| h1.dist_to(**q) <= h2.dist_to(**q));
        (Fitted::new(h1, o1), Fitted::new(h2, o2))
    }

    /// The original samples of `a` followed by those of `b`, thinned to at most `MAX_ORIGINAL`.
    fn merged_original(a: &Fitted, b: &Fitted) -> Vec<P> {
        let mut v = Vec::with_capacity(a.original.len() + b.original.len());
        v.extend_from_slice(&a.original);
        v.extend_from_slice(&b.original);
        while v.len() > MAX_ORIGINAL {
            // keep endpoints, decimate the rest
            let last = *v.last().unwrap();
            v = v.iter().step_by(2).copied().collect();
            if v.last() != Some(&last) {
                v.push(last);
            }
        }
        v
    }
}

impl Path {
    pub fn segs(&self) -> impl ExactSizeIterator<Item = &Seg> {
        self.segs.iter().map(|f| &f.seg)
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    pub fn layer(&self) -> &str {
        &self.layer
    }
    pub fn start(&self) -> P {
        self.segs[0].seg.a
    }
    pub fn end(&self) -> P {
        self.segs.last().unwrap().seg.b
    }
    fn reverse(&mut self) {
        self.segs.reverse();
        for s in &mut self.segs {
            *s = s.reversed();
        }
    }

    /// Appends `next`, meeting this path's end and `next`'s start across the gap.
    fn append(&mut self, next: Path, tol: f64) {
        self.segs = join_segs(std::mem::take(&mut self.segs), next.segs, tol);
    }

    /// Prepends `prev`, meeting `prev`'s end and this path's start across the gap.
    fn prepend(&mut self, prev: Path, tol: f64) {
        self.segs = join_segs(prev.segs, std::mem::take(&mut self.segs), tol);
    }

    /// The path a converted source entity becomes. A closed entity (e.g. a closed polyline
    /// whose last vertex does not repeat the first) gets its closing segment; an open one is
    /// closed if its own ends touch.
    fn from_entity(segs: Vec<Fitted>, closed: bool, layer: String, lim: Limits) -> Path {
        let mut path = Path { segs, closed: false, layer };
        if closed {
            if path.end().dist(path.start()) > negligible(lim.tol) {
                // part of the source geometry, not a bridge
                path.segs.push(Fitted::exact(Seg::new(path.end(), path.start(), 0.0)));
            }
            path.close(lim.tol);
        } else {
            path.close_if_touching(Limits { join_gap: lim.join_gap.max(negligible(lim.tol)), ..lim });
        }
        path
    }

    /// Marks the path closed, meeting its end and start across the remaining gap.
    fn close(&mut self, tol: f64) {
        if self.segs.len() == 1 {
            // a lone segment cannot meet itself: split it so its two halves can
            let (h1, h2) = self.segs[0].halves();
            self.segs = vec![h1, h2];
        }
        let last = self.segs.pop().unwrap();
        let first = self.segs.remove(0);
        let mut joint = meet(last, first, tol);
        self.segs.insert(0, joint.pop().unwrap());
        self.segs.extend(joint);
        self.closed = true;
    }

    /// Closes an open path whose ends are within the join gap of each other, unless it is
    /// too short to be an outline.
    fn close_if_touching(&mut self, lim: Limits) {
        if self.closed || self.segs.is_empty() {
            return;
        }
        let gap = lim.join_gap;
        let len = self.length();
        if self.start().dist(self.end()) <= gap && len > 4.0 * gap && (self.segs.len() >= 2 || !self.segs[0].seg.is_line()) {
            self.close(lim.tol);
        }
    }

    /// Moves the whole path, with its original samples, by `d`.
    fn translate(&mut self, d: P) {
        for f in &mut self.segs {
            f.translate(d);
        }
    }

    /// Whether `other` traces the same outline as this path, within `tol`.
    fn same_outline(&self, other: &Path, tol: f64) -> bool {
        if self.closed != other.closed {
            return false;
        }
        let (b1, b2) = (self.bbox(), other.bbox());
        if b1.min.dist(b2.min) > tol || b1.max.dist(b2.max) > tol {
            return false;
        }
        if (self.length() - other.length()).abs() > tol * (self.segs.len() + other.segs.len()) as f64 {
            return false;
        }
        let covers = |x: &Path, y: &Path| {
            x.segs().all(|s| s.samples(if s.is_line() { 2 } else { 6 }).iter().all(|q| dist_to_segs(*q, y.segs()) <= tol))
        };
        covers(self, other) && covers(other, self)
    }
    pub fn bbox(&self) -> BBox {
        let mut b = BBox::empty();
        for s in self.segs() {
            for q in s.samples(if s.is_line() { 1 } else { 16 }) {
                b.add(q);
            }
        }
        b
    }
    pub fn length(&self) -> f64 {
        self.segs().map(|s| s.length()).sum()
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
            Shape::Path(p) => p.layer(),
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

impl Stats {
    /// Warning for exporting a result that still has open paths, which nesting software
    /// rejects unless they are marking lines. `None` when every path is closed.
    pub fn open_path_warning(&self) -> Option<String> {
        let (heading, consequence) = (self.open_path_heading()?, self.open_path_consequence()?);
        Some(format!("{heading}: {}{}", consequence[..1].to_lowercase(), &consequence[1..]))
    }

    /// The warning's heading, e.g. "1 open path". `None` when every path is closed.
    pub fn open_path_heading(&self) -> Option<String> {
        let n = self.open_paths;
        (n > 0).then(|| format!("{n} open {}", if n == 1 { "path" } else { "paths" }))
    }

    /// The warning's sentence: what happens to a file with open paths.
    pub fn open_path_consequence(&self) -> Option<String> {
        let which = if self.open_paths == 1 { "it is a marking line" } else { "they are marking lines" };
        (self.open_paths > 0).then(|| format!("Nesting software will reject this file unless {which}."))
    }
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
        let s = Seg::new(a, b, bu);
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
    let s1 = Seg::new(p1, pm, bulge_from_start_tangent(p1, pm, t1));
    let s2 = Seg::new(pm, p2, bulge_from_end_tangent(pm, p2, t2));
    Some((s1, s2))
}

/// `segs` with each of `samples` given to the segment nearest to it, between the segment's own ends.
fn assign_original(segs: &[Seg], samples: &[P]) -> Vec<Fitted> {
    let mut out: Vec<Fitted> = segs.iter().map(|s| Fitted::new(*s, vec![s.a])).collect();
    for q in samples {
        let (mut bi, mut bd) = (0, f64::INFINITY);
        for (i, s) in segs.iter().enumerate() {
            let d = s.dist_to(*q);
            if d < bd {
                bd = d;
                bi = i;
            }
        }
        out[bi].original.push(*q);
    }
    for f in &mut out {
        f.original.push(f.seg.b);
    }
    out
}

/// Fit a smooth parametric curve on [t0, t1] with lines/arcs/biarcs.
fn fit_parametric(f: &dyn Fn(f64) -> P, t0: f64, t1: f64, tol: f64, depth: usize, out: &mut Vec<Fitted>) {
    let a = f(t0);
    let b = f(t1);
    let samples: Vec<P> = (0..=SAMPLES).map(|i| f(t0 + (t1 - t0) * i as f64 / SAMPLES as f64)).collect();
    if samples.iter().all(|q| q.dist(a) <= negligible(tol)) {
        return; // degenerate (zero length)
    }
    let split = |out: &mut Vec<Fitted>| {
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
        out.push(Fitted::new(Seg::new(a, b, 0.0), samples));
        return;
    }
    // 2. single arc
    let m = f(0.5 * (t0 + t1));
    let bu0 = bulge_through(a, m, b);
    if bu0.is_finite() {
        let (bu, _) = best_arc(a, b, bu0, &samples, tol);
        let s = Seg::new(a, b, bu);
        if s.sweep().abs() < 2.0 * PI - 1e-3 && max_dev(std::slice::from_ref(&s), &samples) <= tol {
            out.push(Fitted::new(s, samples));
            return;
        }
    }
    // 3. biarc using end tangents
    let h = (t1 - t0) * 1e-5;
    let ta = (f(t0 + h) - a).norm();
    let tb = (b - f(t1 - h)).norm();
    if ta.len() > 0.5 && tb.len() > 0.5 {
        if let Some((s1, s2)) = biarc(a, ta, b, tb) {
            let segs = [s1, s2];
            if max_dev(&segs, &samples) <= tol {
                out.extend(assign_original(&segs, &samples));
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
                out.push(Fitted::new(Seg::new(w[0], w[1], 0.0), vec![w[0], w[1]]));
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
    let s = Seg::new(q[i], q[j], bu);
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
fn fit_points(pts: &[P], tol: f64) -> Vec<Fitted> {
    let mut q: Vec<P> = Vec::with_capacity(pts.len());
    for &x in pts {
        if q.last().map(|l: &P| l.dist(x) > negligible(tol)).unwrap_or(true) {
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
                let original = q[i..=j].to_vec();
                let (bu, _) = best_arc(q[i], q[j], bu, &original, tol);
                out.push(Fitted::new(Seg::new(q[i], q[j], bu), original));
                i = j;
            }
            _ => {
                out.push(Fitted::new(Seg::new(q[i], q[jl], 0.0), q[i..=jl].to_vec()));
                i = jl;
            }
        }
    }
    out
}

// ---------------------------------------------------------------- entity -> segments

fn arc_segs(c: P, r: f64, a0: f64, a1: f64) -> Vec<Fitted> {
    let sweep = a1 - a0;
    let n = (sweep.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let pt = |a: f64| c + p(a.cos(), a.sin()) * r;
    (0..n)
        .map(|i| {
            let s0 = a0 + sweep * i as f64 / n as f64;
            let s1 = a0 + sweep * (i + 1) as f64 / n as f64;
            Fitted::exact(Seg::new(pt(s0), pt(s1), ((s1 - s0) / 4.0).tan()))
        })
        .collect()
}

enum Converted {
    Segs(Vec<Fitted>, bool),
    Circle(P, f64),
}

fn convert_entity(e: &Entity, tol: f64) -> Converted {
    let spans = e.geom.spans();
    if let Some((c, r)) = spans.circle() {
        return Converted::Circle(c, r);
    }
    let mut segs = Vec::new();
    // straight polyline spans in a row are fitted together: they may be a tessellated arc
    let mut run: Vec<P> = Vec::new();
    let flush = |run: &mut Vec<P>, segs: &mut Vec<Fitted>| {
        if run.len() >= 2 {
            segs.extend(fit_points(run, tol));
        }
        run.clear();
    };
    for span in &spans.spans {
        if let Span::Bulge { a, b, .. } = *span {
            if span.is_straight() {
                if run.is_empty() {
                    run.push(a);
                }
                run.push(b);
                continue;
            }
        }
        flush(&mut run, &mut segs);
        fit_span(span, tol, &mut segs);
    }
    flush(&mut run, &mut segs);
    Converted::Segs(segs, spans.closed)
}

/// Segments within the tolerance of one span (a straight polyline span is fitted as part of a run).
fn fit_span(span: &Span, tol: f64, segs: &mut Vec<Fitted>) {
    match *span {
        Span::Line { a, b } => {
            if a.dist(b) > negligible(tol) {
                segs.push(Fitted::exact(Seg::new(a, b, 0.0)));
            }
        }
        Span::Arc { c, r, a0, a1 } => segs.extend(arc_segs(c, r, a0, a1)),
        Span::Circle { c, r } => segs.extend(arc_segs(c, r, 0.0, 2.0 * PI)),
        Span::Ellipse { t0, t1, .. } => {
            let f = |t: f64| span.at(t);
            let n = ((t1 - t0) / (PI / 2.0)).ceil().max(1.0) as usize;
            for i in 0..n {
                let s0 = t0 + (t1 - t0) * i as f64 / n as f64;
                let s1 = t0 + (t1 - t0) * (i + 1) as f64 / n as f64;
                fit_parametric(&f, s0, s1, tol, 0, segs);
            }
        }
        Span::Bulge { a, b, bulge } => {
            if a.dist(b) > negligible(tol) {
                let s = Seg::new(a, b, bulge);
                // split big arcs into segments of at most 90 degrees for robust merging
                match s.arc() {
                    Some(ai) if ai.sweep.abs() > PI / 2.0 + 1e-9 => segs.extend(arc_segs(ai.c, ai.r, ai.a0, ai.a0 + ai.sweep)),
                    _ => segs.push(Fitted::exact(s)),
                }
            }
        }
        Span::Nurbs { t0, t1, .. } => fit_parametric(&|t: f64| span.at(t), t0, t1, tol, 0, segs),
        Span::FitPoints(pts) => segs.extend(fit_points(pts, tol)),
    }
}

// ---------------------------------------------------------------- merging

fn try_merge(f1: &Fitted, f2: &Fitted, tol: f64) -> Option<Fitted> {
    let (s1, s2) = (&f1.seg, &f2.seg);
    let l1 = s1.is_line();
    let l2 = s2.is_line();
    let original = Fitted::merged_original(f1, f2);
    let cand = if l1 && l2 {
        if (s1.b - s1.a).dot(s2.b - s2.a) <= 0.0 {
            return None;
        }
        if s1.a.dist(s2.b) <= tol {
            return None;
        }
        Seg::new(s1.a, s2.b, 0.0)
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
        let (bu, _) = best_arc(s1.a, s2.b, bu0, &original, tol);
        Seg::new(s1.a, s2.b, bu)
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
        let (bu, _) = best_arc(s1.a, s2.b, bu0, &original, tol);
        Seg::new(s1.a, s2.b, bu)
    };
    // validate both ways
    let merged = Fitted::new(cand, original);
    if !merged.traces_original(tol) {
        return None;
    }
    if cand.samples(12).iter().any(|q| dist_to_segs(*q, [s1, s2]) > tol) {
        return None;
    }
    Some(merged)
}

impl Path {
    /// Drops negligible lines and merges collinear lines / co-circular arcs.
    fn simplify(&mut self, tol: f64) {
        let tiny = negligible(tol);
        let mut out: Vec<Fitted> = Vec::with_capacity(self.segs.len());
        // start of a dropped leading piece, handed to the next kept segment
        let mut carried_start: Option<P> = None;
        for mut f in self.segs.drain(..) {
            if f.seg.chord() <= tiny && f.seg.is_line() {
                // drop, but keep continuity
                match out.last_mut() {
                    Some(last) => last.seg.b = f.seg.b,
                    None => carried_start = carried_start.or(Some(f.seg.a)),
                }
                continue;
            }
            if let Some(a) = carried_start.take() {
                f.seg.a = a;
            }
            if let Some(last) = out.last() {
                if let Some(m) = try_merge(last, &f, tol) {
                    *out.last_mut().unwrap() = m;
                    continue;
                }
            }
            out.push(f);
        }
        if self.closed {
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
        self.segs = out;
    }

    /// The circle this closed path traces, if every original sample lies on it within `tol`.
    fn as_circle(&self, tol: f64) -> Option<(P, f64)> {
        if !self.closed || self.segs.is_empty() {
            return None;
        }
        let infos: Vec<ArcInfo> = self.segs().filter_map(|s| s.arc()).collect();
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
        for f in &self.segs {
            for q in f.original.iter().chain([&f.seg.a, &f.seg.b]) {
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
        for f in &self.segs {
            let s = &f.seg;
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
            for q in f.original.iter().chain([&s.a, &s.b]) {
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
}

// ---------------------------------------------------------------- joining

fn join_paths(paths: Vec<Path>, lim: Limits) -> Vec<Path> {
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
                        if d <= lim.join_gap && best.map(|b| d < b.2).unwrap_or(true) {
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
                cur.append(nxt, lim.tol);
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
                        if d <= lim.join_gap && best.map(|b| d < b.2).unwrap_or(true) {
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
                cur.prepend(prv, lim.tol);
            }
        }
        cur.close_if_touching(lim);
        out.push(cur);
    }
    out
}

/// Joins two runs of segments where `left`'s last segment ends near `right`'s first start.
fn join_segs(mut left: Vec<Fitted>, right: Vec<Fitted>, tol: f64) -> Vec<Fitted> {
    let mut right = right.into_iter();
    match (left.pop(), right.next()) {
        (Some(l), Some(r)) => left.extend(meet(l, r, tol)),
        (l, r) => left.extend(l.into_iter().chain(r)),
    }
    left.extend(right);
    left
}

/// Where `left` ends and `right` starts across a gap, the two meet halfway, as long as
/// neither moved segment strays more than `tol` from where it was (checked both ways).
/// Otherwise neither moves and a straight bridge fills the gap, so the path stays continuous
/// and the original geometry stays exactly where it was.
/// Returns `[left, right]` or `[left, bridge, right]`.
fn meet(left: Fitted, right: Fitted, tol: f64) -> Vec<Fitted> {
    let gap = left.seg.b.dist(right.seg.a);
    if gap == 0.0 {
        return vec![left, right];
    }
    let m = left.seg.b.lerp(right.seg.a, 0.5);
    let l2 = Fitted::new(Seg { b: m, ..left.seg }, left.original.clone());
    let r2 = Fitted::new(Seg { a: m, ..right.seg }, right.original.clone());
    if gap <= negligible(tol) || (stays_near(&l2, &left.seg, tol) && stays_near(&r2, &right.seg, tol)) {
        vec![l2, r2]
    } else {
        let b = bridge(left.seg.b, right.seg.a);
        vec![left, b, right]
    }
}

/// Whether `moved` still traces the segment it was `before` it moved, within `tol`: its original
/// samples lie on `moved`, and `moved` lies on `before`.
fn stays_near(moved: &Fitted, before: &Seg, tol: f64) -> bool {
    let s = &moved.seg;
    moved.traces_original(tol) && s.samples(if s.is_line() { 1 } else { 16 }).iter().all(|q| before.dist_to(*q) <= tol)
}

/// A straight segment that fills a gap. It is not traced from the original geometry: its own ends
/// stand in for original samples, so later merges still keep it honest.
fn bridge(a: P, b: P) -> Fitted {
    Fitted::new(Seg::new(a, b, 0.0), vec![a, b])
}

// ---------------------------------------------------------------- duplicates

fn shapes_equal(a: &Shape, b: &Shape, tol: f64) -> bool {
    match (a, b) {
        (Shape::Circle { c: c1, r: r1, .. }, Shape::Circle { c: c2, r: r2, .. }) => {
            c1.dist(*c2) <= tol && (r1 - r2).abs() <= tol
        }
        (Shape::Path(p1), Shape::Path(p2)) => p1.same_outline(p2, tol),
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
    let lim = Limits { tol, join_gap: opt.join_gap.max(0.0) };
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
                shapes.push(Shape::Path(Path::from_entity(segs, closed, layer_of(e), lim)));
            }
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
    if opt.join_paths && lim.join_gap > 0.0 {
        paths = join_paths(paths, lim);
    }

    // 4. merge collinear lines / co-circular arcs, detect circles
    let mut shapes: Vec<Shape> = circles;
    for mut path in paths {
        path.simplify(tol);
        if path.segs.is_empty() {
            continue;
        }
        if let Some((c, r)) = path.as_circle(tol) {
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
            for f in &path.segs {
                for q in &f.original {
                    stats.max_deviation = stats.max_deviation.max(f.seg.dist_to(*q));
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
                    Shape::Path(path) => path.translate(-d),
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
                for seg in p.segs() {
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

/// Lengths and gaps at or below this are treated as zero (a thousandth of the tolerance).
fn negligible(tol: f64) -> f64 {
    tol * 1e-3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf_read::{Drawing, Entity, Geom};

    #[test]
    fn open_path_warning_reads_the_same_whole_or_in_parts() {
        let open = |n| Stats { open_paths: n, ..Stats::default() };
        assert_eq!(open(0).open_path_warning(), None);
        assert_eq!(open(0).open_path_heading(), None);
        assert_eq!(open(0).open_path_consequence(), None);
        assert_eq!(open(1).open_path_heading().as_deref(), Some("1 open path"));
        assert_eq!(open(1).open_path_consequence().as_deref(), Some("Nesting software will reject this file unless it is a marking line."));
        assert_eq!(
            open(1).open_path_warning().as_deref(),
            Some("1 open path: nesting software will reject this file unless it is a marking line.")
        );
        assert_eq!(
            open(3).open_path_warning().as_deref(),
            Some("3 open paths: nesting software will reject this file unless they are marking lines.")
        );
    }

    fn ent(geom: Geom) -> Entity {
        Entity { layer: "0".into(), kind: "TEST", geom }
    }

    #[test]
    fn everything_on_layer_0_can_be_switched_on_and_off() {
        let mut opt = Options::default();
        opt.put_everything_on_layer_0(true);
        assert_eq!(opt.single_layer.as_deref(), Some("0"));
        opt.put_everything_on_layer_0(false);
        assert_eq!(opt.single_layer, None, "back to the source layers");
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
        // arcs only approximate the free-form curve, and the report must say by how much
        assert!(res.stats.max_deviation > 0.0, "{:?}", res.stats);
        // free-form curve: arcs only, tangent-continuous
        let path = res.shapes.iter().find_map(|s| if let Shape::Path(p) = s { Some(p) } else { None }).unwrap();
        assert!(path.segs.len() < 40, "too many segments: {}", path.segs.len());
        for w in path.segs.windows(2) {
            let turn = signed_angle(w[0].seg.end_tangent(), w[1].seg.start_tangent()).abs();
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

    fn on_circle(r: f64, deg: f64) -> P {
        let a = deg.to_radians();
        p(r * a.cos(), r * a.sin())
    }

    /// A 120° arc plus the line that closes it, with a 0.2 radial gap at the arc start.
    /// The Join gap (0.3) is wider than the Tolerance (0.05), as staff set it when
    /// paths stay open.
    fn arc_and_line_with_gap() -> (Drawing, Options) {
        let r = 20.0;
        let line = ent(Geom::Line { a: on_circle(r, 120.0), b: on_circle(r, 0.0) + p(0.2, 0.0) });
        let arc = ent(Geom::Arc { c: p(0.0, 0.0), r, a0: 0.0, a1: 120f64.to_radians() });
        let d = Drawing { entities: vec![line, arc], ..Default::default() };
        (d, Options { join_gap: 0.3, ..Options::default() })
    }

    #[test]
    fn wide_join_gap_closes_without_moving_geometry_past_tolerance() {
        let (d, opt) = arc_and_line_with_gap();
        let res = clean(&d, &opt);
        assert_eq!((res.stats.closed_paths, res.stats.open_paths), (1, 0), "{:?}", res.stats);
        assert!(res.stats.max_deviation <= opt.tolerance + 1e-9, "{:?}", res.stats);
    }

    #[test]
    fn half_arcs_with_small_gaps_meet_and_become_circle() {
        let r = 20.0;
        let upper = ent(Geom::Arc { c: p(0.0, 0.0), r, a0: 0.0, a1: PI });
        // lower half, nudged so both joints have a 0.01 gap
        let lower = ent(Geom::Arc { c: p(0.0, 0.01), r, a0: PI, a1: 2.0 * PI });
        let d = Drawing { entities: vec![upper, lower], ..Default::default() };
        let opt = Options::default();
        let res = clean(&d, &opt);
        assert_eq!(res.stats.circles, 1, "{:?}", res.stats);
        assert!(res.stats.max_deviation <= opt.tolerance + 1e-9, "{:?}", res.stats);
    }

    #[test]
    fn closing_never_leaves_a_zero_length_or_unclosed_end() {
        // every closed path must end exactly where it starts (the DXF writer relies on it)
        let (d, opt) = arc_and_line_with_gap();
        let res = clean(&d, &opt);
        for s in &res.shapes {
            if let Shape::Path(path) = s {
                assert_exactly_closed(path);
                assert!(path.segs().all(|s| s.chord() > 0.0));
            }
        }
    }

    fn assert_exactly_closed(path: &Path) {
        assert!(path.closed);
        assert_eq!(path.end(), path.start(), "closed path must end where it starts");
        for w in path.segs.windows(2) {
            assert_eq!(w[0].seg.b, w[1].seg.a, "segments must be continuous");
        }
    }

    fn only_path(res: &CleanResult) -> &Path {
        let paths: Vec<&Path> = res.shapes.iter().filter_map(|s| if let Shape::Path(p) = s { Some(p) } else { None }).collect();
        assert_eq!(paths.len(), 1, "{:?}", res.stats);
        paths[0]
    }

    #[test]
    fn closed_polyline_with_tiny_closing_gap_closes_exactly() {
        // last vertex repeats the first up to 1e-5 (as exporters often write it)
        let pts = vec![p(0.0, 0.0), p(30.0, 0.0), p(30.0, 20.0), p(0.0, 20.0), p(1e-5, 0.0)];
        let d = Drawing { entities: vec![ent(Geom::Poly { pts, bulges: vec![0.0; 5], closed: true })], ..Default::default() };
        let res = clean(&d, &Options::default());
        assert_eq!((res.stats.closed_paths, res.stats.lines), (1, 4), "{:?}", res.stats);
        assert_exactly_closed(only_path(&res));
    }

    #[test]
    fn corner_gap_is_bridged_and_original_corner_stays_put() {
        // rectangle of lines; the last edge stops 0.2 short of the first corner
        let (a, b, c, e) = (p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0));
        let ents = vec![
            ent(Geom::Line { a, b }),
            ent(Geom::Line { a: b, b: c }),
            ent(Geom::Line { a: c, b: e }),
            ent(Geom::Line { a: e, b: p(0.0, 0.2) }),
        ];
        let d = Drawing { entities: ents, ..Default::default() };
        let opt = Options { join_gap: 0.3, ..Options::default() };
        let res = clean(&d, &opt);
        assert_eq!((res.stats.closed_paths, res.stats.open_paths), (1, 0), "{:?}", res.stats);
        assert!(res.stats.max_deviation <= opt.tolerance + 1e-9, "{:?}", res.stats);
        let path = only_path(&res);
        assert_exactly_closed(path);
        // meeting halfway would have cut the corner; the bridge keeps (0,0) as a vertex
        assert!(path.segs().any(|s| s.a == a), "corner moved: {:?}", path.segs);
    }

    #[test]
    fn collinear_lines_across_wide_gap_join_into_one_line() {
        let ents = vec![ent(Geom::Line { a: p(0.0, 0.0), b: p(10.0, 0.0) }), ent(Geom::Line { a: p(10.2, 0.0), b: p(20.0, 0.0) })];
        let d = Drawing { entities: ents, ..Default::default() };
        let opt = Options { join_gap: 0.3, ..Options::default() };
        let res = clean(&d, &opt);
        assert_eq!((res.stats.open_paths, res.stats.lines), (1, 1), "{:?}", res.stats);
        let path = only_path(&res);
        assert_eq!((path.start(), path.end()), (p(0.0, 0.0), p(20.0, 0.0)));
    }

    // Not reachable through clean() today (arcs are split into <= 90° pieces before joining),
    // so this guards Path::close directly.
    #[test]
    fn lone_segment_closes_exactly() {
        let r = 10.0;
        let arc = Seg::new(on_circle(r, 0.0), on_circle(r, 359.0), (359f64.to_radians() / 4.0).tan());
        for seg in [arc, Seg { b: arc.a + p(1e-6, 0.0), ..arc }] {
            let mut path = Path { segs: vec![Fitted::exact(seg)], closed: false, layer: "0".into() };
            path.close(0.05);
            assert_exactly_closed(&path);
            assert!(path.segs().all(|s| s.chord() > 0.0), "{:?}", path.segs);
        }
    }

    #[test]
    fn full_circles_keep_their_exact_centre_and_radius() {
        let c = p(12.345678901, -6.789012345);
        let r = 7.123456789;
        let ents = vec![
            ent(Geom::Circle { c, r }),
            ent(Geom::Arc { c: c + p(100.0, 0.0), r, a0: 0.3, a1: 0.3 + 2.0 * PI }),
            ent(Geom::Ellipse { c: c + p(200.0, 0.0), u: p(0.0, r), v: p(-r, 0.0), t0: 0.0, t1: 2.0 * PI }),
        ];
        let res = clean(&Drawing { entities: ents, ..Default::default() }, &Options::default());
        let mut circles: Vec<(P, f64)> = res
            .shapes
            .iter()
            .map(|s| match s {
                Shape::Circle { c, r, .. } => (*c, *r),
                other => panic!("not a circle: {other:?}"),
            })
            .collect();
        circles.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        assert_eq!(circles, vec![(c, r), (c + p(100.0, 0.0), r), (c + p(200.0, 0.0), r)]);
    }

    /// A quarter of the circle of radius 10 around the origin, counter-clockwise from (10, 0).
    fn quarter_arc() -> Seg {
        Seg::new(p(10.0, 0.0), p(0.0, 10.0), (PI / 8.0).tan())
    }

    #[test]
    fn exact_segments_sample_their_own_geometry() {
        let line = Fitted::exact(Seg::new(p(0.0, 0.0), p(4.0, 0.0), 0.0));
        assert_eq!(line.original, vec![p(0.0, 0.0), p(2.0, 0.0), p(4.0, 0.0)]);
        let arc = Fitted::exact(quarter_arc());
        assert_eq!(arc.original.len(), 9);
        assert!(arc.original[0].dist(arc.seg.a) < 1e-9 && arc.original[8].dist(arc.seg.b) < 1e-9, "{:?}", arc.original);
        assert!(arc.original.iter().all(|q| (q.len() - 10.0).abs() < 1e-9), "{:?}", arc.original);
    }

    #[test]
    fn reversed_runs_the_original_samples_backwards() {
        let f = Fitted::exact(quarter_arc());
        let r = f.reversed();
        assert_eq!((r.seg.a, r.seg.b, r.seg.bulge), (f.seg.b, f.seg.a, -f.seg.bulge));
        let mut back = r.original.clone();
        back.reverse();
        assert_eq!(back, f.original);
    }

    #[test]
    fn translate_moves_geometry_and_original_samples_together() {
        let before = Fitted::exact(quarter_arc());
        let d = p(3.0, -2.0);
        let mut f = before.clone();
        f.translate(d);
        assert_eq!((f.seg.a, f.seg.b, f.seg.bulge), (before.seg.a + d, before.seg.b + d, before.seg.bulge));
        assert_eq!(f.original, before.original.iter().map(|q| *q + d).collect::<Vec<P>>());
    }

    #[test]
    fn halves_give_each_original_sample_to_the_nearest_half() {
        let f = Fitted::exact(quarter_arc());
        let (h1, h2) = f.halves();
        assert_eq!((h1.seg.a, h1.seg.b, h2.seg.b), (f.seg.a, h2.seg.a, f.seg.b));
        assert_eq!(h1.original.len() + h2.original.len(), f.original.len());
        for q in &f.original {
            let (in1, in2) = (h1.original.contains(q), h2.original.contains(q));
            assert!(in1 != in2, "{q:?} must be in exactly one half");
            let (d1, d2) = (h1.seg.dist_to(*q), h2.seg.dist_to(*q));
            assert!(if in1 { d1 <= d2 } else { d2 < d1 }, "{q:?} went to the further half");
        }
    }

    #[test]
    fn merged_original_samples_are_thinned_but_keep_both_ends() {
        let run = |x0: f64| -> Fitted {
            let pts: Vec<P> = (0..300).map(|i| p(x0 + i as f64, 0.0)).collect();
            Fitted::new(Seg::new(pts[0], pts[299], 0.0), pts)
        };
        let m = Fitted::merged_original(&run(0.0), &run(300.0));
        assert!(m.len() <= MAX_ORIGINAL, "{} samples", m.len());
        assert_eq!((m[0], *m.last().unwrap()), (p(0.0, 0.0), p(599.0, 0.0)));
    }

    #[test]
    fn traces_original_fails_for_a_far_or_unmeasurable_sample() {
        let mut f = Fitted::exact(Seg::new(p(0.0, 0.0), p(10.0, 0.0), 0.0));
        assert!(f.traces_original(0.05));
        f.original.push(p(5.0, 0.06));
        assert!(!f.traces_original(0.05));
        f.original.pop();
        f.original.push(p(f64::NAN, 0.0));
        assert!(!f.traces_original(0.05));
    }

    #[test]
    fn assign_original_gives_each_segment_its_nearest_samples_between_its_ends() {
        let segs = [Seg::new(p(0.0, 0.0), p(5.0, 0.0), 0.0), Seg::new(p(5.0, 0.0), p(10.0, 0.0), 0.0)];
        let fitted = assign_original(&segs, &[p(1.0, 0.0), p(7.0, 0.0)]);
        let originals: Vec<&[P]> = fitted.iter().map(|f| f.original.as_slice()).collect();
        assert_eq!(originals, [[p(0.0, 0.0), p(1.0, 0.0), p(5.0, 0.0)], [p(5.0, 0.0), p(7.0, 0.0), p(10.0, 0.0)]]);
    }

    /// Two lines whose ends are 0.06 apart in y, and the left line's original samples. The ends
    /// can meet halfway (0.03 each) without the lines themselves leaving the tolerance (0.05).
    fn lines_across_a_gap(left_original: Vec<P>) -> (Fitted, Fitted) {
        let left = Fitted::new(Seg::new(p(0.0, 0.0), p(10.0, 0.0), 0.0), left_original);
        (left, Fitted::exact(Seg::new(p(10.0, 0.06), p(20.0, 0.06), 0.0)))
    }

    #[test]
    fn meeting_halfway_keeps_the_original_samples() {
        let original = vec![p(0.0, 0.0), p(9.0, 0.02), p(10.0, 0.0)];
        let (left, right) = lines_across_a_gap(original.clone());
        let met = meet(left, right.clone(), 0.05);
        assert_eq!(met.len(), 2, "expected the ends to meet: {met:?}");
        assert_eq!((met[0].seg.b, met[1].seg.a), (p(10.0, 0.03), p(10.0, 0.03)));
        assert_eq!((&met[0].original, &met[1].original), (&original, &right.original));
    }

    #[test]
    fn ends_do_not_meet_when_an_original_sample_would_stray() {
        // (9, -0.04) is within tolerance of the left line, but 0.067 from it once its end moves up
        let (left, right) = lines_across_a_gap(vec![p(0.0, 0.0), p(9.0, -0.04), p(10.0, 0.0)]);
        let met = meet(left, right, 0.05);
        assert_eq!(met.len(), 3, "expected a bridge: {met:?}");
        assert_eq!((met[0].seg.b, met[2].seg.a), (p(10.0, 0.0), p(10.0, 0.06)), "original ends moved");
    }

    #[test]
    fn merge_is_refused_when_an_original_sample_would_stray() {
        // two lines with a 0.04 kink: the straight line over both stays within 0.05 of them
        let kink = |off_sample: P| {
            let f1 = Fitted::new(Seg::new(p(0.0, 0.0), p(10.0, 0.04), 0.0), vec![p(0.0, 0.0), off_sample, p(10.0, 0.04)]);
            let f2 = Fitted::exact(Seg::new(p(10.0, 0.04), p(20.0, 0.0), 0.0));
            try_merge(&f1, &f2, 0.05)
        };
        assert!(kink(p(5.0, 0.02)).is_some(), "samples on the lines must allow the merge");
        // (5, 0.06) is within tolerance of the first line, but 0.06 from the merged line
        assert!(kink(p(5.0, 0.06)).is_none());
    }

    #[test]
    fn zero_length_lines_are_dropped() {
        let q = p(3.0, 4.0);
        let res = clean(&Drawing { entities: vec![ent(Geom::Line { a: q, b: q })], ..Default::default() }, &Options::default());
        assert!(res.shapes.is_empty(), "{:?}", res.shapes);
    }
}
