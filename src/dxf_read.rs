//! Minimal, tolerant ASCII DXF reader. Extracts 2D cutting geometry
//! (LINE, ARC, CIRCLE, ELLIPSE, LWPOLYLINE, POLYLINE, SPLINE, INSERT) in world coordinates.

use crate::geom::{p, Affine, LINE_BULGE_EPS, P};
use crate::spans::{polyline_spans, Span};
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub enum Geom {
    Line { a: P, b: P },
    /// CCW arc, angles in radians.
    Arc { c: P, r: f64, a0: f64, a1: f64 },
    Circle { c: P, r: f64 },
    /// f(t) = c + u cos t + v sin t, for t in [t0, t1], t0 < t1.
    Ellipse { c: P, u: P, v: P, t0: f64, t1: f64 },
    /// `bulges.len() == pts.len()`; the last bulge is used only if closed.
    Poly { pts: Vec<P>, bulges: Vec<f64>, closed: bool },
    Spline { degree: usize, knots: Vec<f64>, ctrl: Vec<P>, weights: Vec<f64>, fit: Vec<P>, closed: bool },
}

#[derive(Clone, Debug)]
pub struct Entity {
    pub layer: String,
    /// Source entity type name (for statistics), e.g. "SPLINE".
    pub kind: &'static str,
    pub geom: Geom,
}

#[derive(Clone, Debug, Default)]
pub struct Drawing {
    pub entities: Vec<Entity>,
    /// Entity types that were ignored (not cuttable), with counts.
    pub skipped: BTreeMap<String, usize>,
    /// Count of read geometry entities by type.
    pub counts: BTreeMap<String, usize>,
    pub insunits: Option<i32>,
    pub version: Option<String>,
}

#[derive(Clone, Debug)]
struct Raw {
    kind: String,
    pairs: Vec<(i32, String)>,
    /// VERTEX children for POLYLINE.
    children: Vec<Raw>,
}

impl Raw {
    fn get(&self, code: i32) -> Option<&str> {
        self.pairs.iter().find(|(c, _)| *c == code).map(|(_, v)| v.as_str())
    }
    fn f(&self, code: i32, default: f64) -> f64 {
        self.get(code).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(default)
    }
    fn i(&self, code: i32, default: i64) -> i64 {
        self.get(code)
            .and_then(|v| {
                let v = v.trim();
                v.parse::<i64>().ok().or_else(|| v.parse::<f64>().ok().map(|f| f as i64))
            })
            .unwrap_or(default)
    }
    fn layer(&self) -> String {
        self.get(8).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "0".into())
    }
    fn extrusion(&self) -> [f64; 3] {
        [self.f(210, 0.0), self.f(220, 0.0), self.f(230, 1.0)]
    }
}

fn parse_pairs(text: &str) -> Result<Vec<(i32, String)>, String> {
    let mut lines = text.lines();
    let mut out = Vec::new();
    loop {
        let Some(code_line) = lines.next() else { break };
        let code_s = code_line.trim();
        if code_s.is_empty() {
            // tolerate blank trailing lines
            continue;
        }
        let code: i32 = code_s
            .parse()
            .map_err(|_| format!("Invalid group code '{code_s}' at pair {}", out.len() + 1))?;
        let value = lines.next().unwrap_or("").trim_end_matches(['\r', '\n']).to_string();
        let v = if code == 0 || code == 2 || code == 8 { value.trim().to_string() } else { value };
        out.push((code, v));
        if code == 0 && out.last().map(|x| x.1.as_str()) == Some("EOF") {
            break;
        }
    }
    Ok(out)
}

/// Split a run of pairs (starting at a code-0 pair) into entity records.
fn split_entities(pairs: &[(i32, String)]) -> Vec<Raw> {
    let mut raws: Vec<Raw> = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        if pairs[i].0 != 0 {
            i += 1;
            continue;
        }
        let kind = pairs[i].1.clone();
        let mut j = i + 1;
        while j < pairs.len() && pairs[j].0 != 0 {
            j += 1;
        }
        raws.push(Raw { kind, pairs: pairs[i + 1..j].to_vec(), children: Vec::new() });
        i = j;
    }
    // Attach VERTEX/ATTRIB sequences to their owners.
    let mut out: Vec<Raw> = Vec::new();
    let mut it = raws.into_iter().peekable();
    while let Some(mut r) = it.next() {
        if r.kind == "POLYLINE" {
            while let Some(n) = it.peek() {
                if n.kind == "VERTEX" {
                    r.children.push(it.next().unwrap());
                } else {
                    break;
                }
            }
            if it.peek().map(|n| n.kind == "SEQEND").unwrap_or(false) {
                it.next();
            }
        } else if r.kind == "INSERT" {
            // skip attributes
            let mut had_attr = false;
            while let Some(n) = it.peek() {
                if n.kind == "ATTRIB" {
                    had_attr = true;
                    it.next();
                } else {
                    break;
                }
            }
            if had_attr && it.peek().map(|n| n.kind == "SEQEND").unwrap_or(false) {
                it.next();
            }
        }
        out.push(r);
    }
    out
}

struct Block {
    base: P,
    ents: Vec<Raw>,
}

pub fn read_file(path: &std::path::Path) -> Result<Drawing, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    read_bytes(&bytes)
}

pub fn read_bytes(bytes: &[u8]) -> Result<Drawing, String> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err("Binary DXF is not supported. Please export as ASCII DXF.".into());
    }
    let text = String::from_utf8_lossy(bytes);
    let pairs = parse_pairs(&text)?;

    let mut dwg = Drawing::default();
    let mut blocks: HashMap<String, Block> = HashMap::new();
    let mut entity_raws: Vec<Raw> = Vec::new();

    let mut i = 0;
    while i < pairs.len() {
        if pairs[i] == (0, "SECTION".to_string()) && i + 1 < pairs.len() && pairs[i + 1].0 == 2 {
            let name = pairs[i + 1].1.clone();
            let start = i + 2;
            let mut end = start;
            while end < pairs.len() && pairs[end] != (0, "ENDSEC".to_string()) {
                end += 1;
            }
            let body = &pairs[start..end];
            match name.as_str() {
                "HEADER" => read_header(body, &mut dwg),
                "ENTITIES" => entity_raws.extend(split_entities(body)),
                "BLOCKS" => read_blocks(body, &mut blocks),
                _ => {}
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }

    let mut ctx = Ctx { blocks: &blocks, dwg: &mut dwg };
    for r in &entity_raws {
        ctx.convert(r, &Affine::identity(), 0);
    }
    Ok(dwg)
}

fn read_header(body: &[(i32, String)], dwg: &mut Drawing) {
    let mut k = 0;
    while k < body.len() {
        if body[k].0 == 9 {
            let var = body[k].1.trim();
            if let Some(next) = body.get(k + 1) {
                match var {
                    "$INSUNITS" => dwg.insunits = next.1.trim().parse().ok(),
                    "$ACADVER" => dwg.version = Some(next.1.trim().to_string()),
                    _ => {}
                }
            }
        }
        k += 1;
    }
}

fn read_blocks(body: &[(i32, String)], blocks: &mut HashMap<String, Block>) {
    let raws = split_entities(body);
    let mut cur: Option<(String, Block)> = None;
    for r in raws {
        match r.kind.as_str() {
            "BLOCK" => {
                let name = r.get(2).or(r.get(3)).unwrap_or("").trim().to_string();
                cur = Some((name, Block { base: p(r.f(10, 0.0), r.f(20, 0.0)), ents: Vec::new() }));
            }
            "ENDBLK" => {
                if let Some((n, b)) = cur.take() {
                    blocks.insert(n.to_uppercase(), b);
                }
            }
            _ => {
                if let Some((_, b)) = cur.as_mut() {
                    b.ents.push(r);
                }
            }
        }
    }
}

/// Arbitrary-axis algorithm: OCS -> WCS, projected to XY.
fn ocs_affine(n: [f64; 3], elevation: f64) -> Affine {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-12 {
        return Affine::identity();
    }
    let n = [n[0] / len, n[1] / len, n[2] / len];
    if (n[2] - 1.0).abs() < 1e-12 && elevation == 0.0 {
        return Affine::identity();
    }
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let unit = |a: [f64; 3]| {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        unit(cross([0.0, 1.0, 0.0], n))
    } else {
        unit(cross([0.0, 0.0, 1.0], n))
    };
    let ay = unit(cross(n, ax));
    Affine { m: [ax[0], ay[0], ax[1], ay[1]], t: p(n[0] * elevation, n[1] * elevation) }
}

struct Ctx<'a> {
    blocks: &'a HashMap<String, Block>,
    dwg: &'a mut Drawing,
}

impl Ctx<'_> {
    fn skip(&mut self, kind: &str) {
        *self.dwg.skipped.entry(kind.to_string()).or_default() += 1;
    }

    fn push(&mut self, layer: String, kind: &'static str, geom: Geom, xf: &Affine) {
        *self.dwg.counts.entry(kind.to_string()).or_default() += 1;
        for g in transform_geom(geom, xf) {
            self.dwg.entities.push(Entity { layer: layer.clone(), kind, geom: g });
        }
    }

    fn convert(&mut self, r: &Raw, xf: &Affine, depth: usize) {
        let layer = r.layer();
        match r.kind.as_str() {
            "LINE" => {
                let g = Geom::Line { a: p(r.f(10, 0.0), r.f(20, 0.0)), b: p(r.f(11, 0.0), r.f(21, 0.0)) };
                // LINE is in WCS; extrusion only affects thickness.
                self.push(layer, "LINE", g, xf);
            }
            "ARC" | "CIRCLE" => {
                let ocs = ocs_affine(r.extrusion(), 0.0).then(xf);
                let c = p(r.f(10, 0.0), r.f(20, 0.0));
                let rad = r.f(40, 0.0);
                if rad <= 0.0 {
                    return;
                }
                if r.kind == "CIRCLE" {
                    self.push(layer, "CIRCLE", Geom::Circle { c, r: rad }, &ocs);
                } else {
                    let a0 = r.f(50, 0.0).to_radians();
                    let mut a1 = r.f(51, 360.0).to_radians();
                    while a1 <= a0 {
                        a1 += 2.0 * PI;
                    }
                    self.push(layer, "ARC", Geom::Arc { c, r: rad, a0, a1 }, &ocs);
                }
            }
            "ELLIPSE" => {
                let c = p(r.f(10, 0.0), r.f(20, 0.0));
                let maj3 = [r.f(11, 0.0), r.f(21, 0.0), r.f(31, 0.0)];
                let ratio = r.f(40, 1.0);
                let n = r.extrusion();
                let minor3 = [
                    (n[1] * maj3[2] - n[2] * maj3[1]) * ratio,
                    (n[2] * maj3[0] - n[0] * maj3[2]) * ratio,
                    (n[0] * maj3[1] - n[1] * maj3[0]) * ratio,
                ];
                let t0 = r.f(41, 0.0);
                let mut t1 = r.f(42, 2.0 * PI);
                while t1 <= t0 {
                    t1 += 2.0 * PI;
                }
                let g = Geom::Ellipse { c, u: p(maj3[0], maj3[1]), v: p(minor3[0], minor3[1]), t0, t1 };
                self.push(layer, "ELLIPSE", g, xf);
            }
            "LWPOLYLINE" => {
                let mut pts = Vec::new();
                let mut bulges = Vec::new();
                let mut cur_x: Option<f64> = None;
                for (code, v) in &r.pairs {
                    let val = v.trim().parse::<f64>().unwrap_or(0.0);
                    match code {
                        10 => cur_x = Some(val),
                        20 => {
                            if let Some(x) = cur_x.take() {
                                pts.push(p(x, val));
                                bulges.push(0.0);
                            }
                        }
                        42 => {
                            if let Some(b) = bulges.last_mut() {
                                *b = val;
                            }
                        }
                        _ => {}
                    }
                }
                let closed = r.i(70, 0) & 1 != 0;
                let ocs = ocs_affine(r.extrusion(), r.f(38, 0.0)).then(xf);
                if pts.len() >= 2 {
                    self.push(layer, "LWPOLYLINE", Geom::Poly { pts, bulges, closed }, &ocs);
                }
            }
            "POLYLINE" => {
                let flags = r.i(70, 0);
                if flags & (16 | 64) != 0 {
                    self.skip("POLYLINE (mesh)");
                    return;
                }
                let is3d = flags & 8 != 0;
                let mut pts = Vec::new();
                let mut bulges = Vec::new();
                for v in &r.children {
                    let vf = v.i(70, 0);
                    if vf & 16 != 0 {
                        continue; // spline frame control point
                    }
                    pts.push(p(v.f(10, 0.0), v.f(20, 0.0)));
                    bulges.push(if is3d { 0.0 } else { v.f(42, 0.0) });
                }
                let closed = flags & 1 != 0;
                let ocs = if is3d { *xf } else { ocs_affine(r.extrusion(), r.f(30, 0.0)).then(xf) };
                if pts.len() >= 2 {
                    self.push(layer, "POLYLINE", Geom::Poly { pts, bulges, closed }, &ocs);
                }
            }
            "SPLINE" => {
                let flags = r.i(70, 0);
                let degree = r.i(71, 3).max(1) as usize;
                let mut knots = Vec::new();
                let mut weights = Vec::new();
                let mut ctrl = Vec::new();
                let mut fit = Vec::new();
                let mut cx: Option<f64> = None;
                let mut fx: Option<f64> = None;
                for (code, v) in &r.pairs {
                    let val = v.trim().parse::<f64>().unwrap_or(0.0);
                    match code {
                        40 => knots.push(val),
                        41 => weights.push(val),
                        10 => cx = Some(val),
                        20 => {
                            if let Some(x) = cx.take() {
                                ctrl.push(p(x, val));
                            }
                        }
                        11 => fx = Some(val),
                        21 => {
                            if let Some(x) = fx.take() {
                                fit.push(p(x, val));
                            }
                        }
                        _ => {}
                    }
                }
                if weights.len() != ctrl.len() {
                    weights = vec![1.0; ctrl.len()];
                }
                let closed = flags & 1 != 0;
                let valid_nurbs = ctrl.len() > degree && knots.len() == ctrl.len() + degree + 1;
                if !valid_nurbs {
                    ctrl.clear();
                    knots.clear();
                    weights.clear();
                }
                if ctrl.is_empty() && fit.len() < 2 {
                    self.skip("SPLINE (invalid)");
                    return;
                }
                self.push(layer, "SPLINE", Geom::Spline { degree, knots, ctrl, weights, fit, closed }, xf);
            }
            "INSERT" => {
                if depth > 16 {
                    return;
                }
                let name = r.get(2).unwrap_or("").trim().to_uppercase();
                let blocks = self.blocks;
                let Some(block) = blocks.get(&name) else {
                    self.skip("INSERT (missing block)");
                    return;
                };
                let sx = r.f(41, 1.0);
                let sy = r.f(42, 1.0);
                let rot = r.f(50, 0.0).to_radians();
                let cols = r.i(70, 1).max(1);
                let rows = r.i(71, 1).max(1);
                let dc = r.f(44, 0.0);
                let dr = r.f(45, 0.0);
                let ins = p(r.f(10, 0.0), r.f(20, 0.0));
                let ocs = ocs_affine(r.extrusion(), 0.0);
                let (s, c) = rot.sin_cos();
                for row in 0..rows {
                    for col in 0..cols {
                        // block -> local: translate(-base), scale, rotate, translate(ins + array offset)
                        let off = crate::geom::rotate(p(col as f64 * dc, row as f64 * dr), rot);
                        let local = Affine {
                            m: [c * sx, -s * sy, s * sx, c * sy],
                            t: p(0.0, 0.0),
                        };
                        let t = ins + off - local.apply_vec(block.base);
                        let local = Affine { t, ..local };
                        let full = local.then(&ocs).then(xf);
                        for e in &block.ents {
                            let mut e2 = e.clone();
                            // entities on layer "0" inherit the INSERT's layer
                            if e2.layer() == "0" {
                                e2.pairs.retain(|(c, _)| *c != 8);
                                e2.pairs.push((8, layer.clone()));
                            }
                            self.convert(&e2, &full, depth + 1);
                        }
                    }
                }
            }
            "ATTRIB" | "ATTDEF" | "SEQEND" | "VERTEX" | "VIEWPORT" => {}
            other => self.skip(other),
        }
    }
}

/// Apply an affine transform to a geometry, converting arcs to ellipses under non-uniform scaling.
pub fn transform_geom(g: Geom, xf: &Affine) -> Vec<Geom> {
    if xf.is_identity() {
        return vec![g];
    }
    let sim = xf.similarity_scale();
    let mirrored = xf.det() < 0.0;
    match g {
        Geom::Line { a, b } => vec![Geom::Line { a: xf.apply(a), b: xf.apply(b) }],
        Geom::Circle { c, r } => match sim {
            Some(s) => vec![Geom::Circle { c: xf.apply(c), r: r * s }],
            None => vec![Geom::Ellipse {
                c: xf.apply(c),
                u: xf.apply_vec(p(r, 0.0)),
                v: xf.apply_vec(p(0.0, r)),
                t0: 0.0,
                t1: 2.0 * PI,
            }],
        },
        Geom::Arc { c, r, a0, a1 } => match sim {
            Some(s) => {
                let pa = xf.apply(c + p(a0.cos(), a0.sin()) * r);
                let pb = xf.apply(c + p(a1.cos(), a1.sin()) * r);
                let nc = xf.apply(c);
                let (mut b0, mut b1) = ((pa - nc).angle(), (pb - nc).angle());
                if mirrored {
                    std::mem::swap(&mut b0, &mut b1);
                }
                let sweep = a1 - a0;
                let b0n = b0;
                let b1n = b0n + sweep;
                let _ = b1;
                vec![Geom::Arc { c: nc, r: r * s, a0: b0n, a1: b1n }]
            }
            None => vec![Geom::Ellipse {
                c: xf.apply(c),
                u: xf.apply_vec(p(r, 0.0)),
                v: xf.apply_vec(p(0.0, r)),
                t0: a0,
                t1: a1,
            }],
        },
        Geom::Ellipse { c, u, v, t0, t1 } => {
            vec![Geom::Ellipse { c: xf.apply(c), u: xf.apply_vec(u), v: xf.apply_vec(v), t0, t1 }]
        }
        Geom::Poly { pts, bulges, closed } => {
            let has_arcs = bulges.iter().any(|b| b.abs() >= LINE_BULGE_EPS);
            if sim.is_some() || !has_arcs {
                let sign = if mirrored { -1.0 } else { 1.0 };
                vec![Geom::Poly {
                    pts: pts.iter().map(|q| xf.apply(*q)).collect(),
                    bulges: bulges.iter().map(|b| b * sign).collect(),
                    closed,
                }]
            } else {
                // Non-uniform scale: tessellate bulged segments; the cleaner refits them.
                let mut out_pts = Vec::new();
                for span in polyline_spans(&pts, &bulges, closed) {
                    let Span::Bulge { a, b, bulge } = span else { unreachable!("polylines are made of bulge spans") };
                    let s = crate::geom::Seg::new(a, b, bulge);
                    let steps = if s.is_line() { 1 } else { 64 };
                    let smp = s.samples(steps);
                    out_pts.extend_from_slice(&smp[..smp.len() - 1]);
                }
                if !closed {
                    out_pts.push(pts[pts.len() - 1]);
                }
                let bl = vec![0.0; out_pts.len()];
                vec![Geom::Poly { pts: out_pts.iter().map(|q| xf.apply(*q)).collect(), bulges: bl, closed }]
            }
        }
        Geom::Spline { degree, knots, ctrl, weights, fit, closed } => vec![Geom::Spline {
            degree,
            knots,
            ctrl: ctrl.iter().map(|q| xf.apply(*q)).collect(),
            weights,
            fit: fit.iter().map(|q| xf.apply(*q)).collect(),
            closed,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DXF with `blocks` in its BLOCKS section and `entities` in its ENTITIES section.
    fn dxf(blocks: &str, entities: &str) -> Drawing {
        let text = format!("0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n");
        read_bytes(text.as_bytes()).unwrap()
    }

    fn block(name: &str, body: &str) -> String {
        format!("0\nBLOCK\n8\n0\n2\n{name}\n70\n0\n10\n0\n20\n0\n{body}0\nENDBLK\n")
    }

    fn only(d: &Drawing) -> &Geom {
        assert_eq!(d.entities.len(), 1, "{:?}", d.entities);
        &d.entities[0].geom
    }

    fn near(a: P, b: P) -> bool {
        a.dist(b) < 1e-9
    }

    fn on_arc(c: P, r: f64, a: f64) -> P {
        c + p(a.cos(), a.sin()) * r
    }

    #[test]
    fn mirrored_insert_keeps_an_arc_on_the_mirrored_points() {
        // right half of a circle around (10, 10), mirrored about the insert point's y axis
        let arc = "0\nARC\n8\n0\n10\n10\n20\n10\n40\n10\n50\n270\n51\n90\n";
        let d = dxf(&block("B", arc), "0\nINSERT\n8\nCUT\n2\nB\n10\n100\n20\n0\n41\n-1\n42\n1\n");
        let Geom::Arc { c, r, a0, a1 } = *only(&d) else { panic!("{:?}", d.entities) };
        assert!(near(c, p(90.0, 10.0)) && (r - 10.0).abs() < 1e-9);
        let ends = [on_arc(c, r, a0), on_arc(c, r, a1)];
        assert!(ends.iter().any(|q| near(*q, p(90.0, 0.0))) && ends.iter().any(|q| near(*q, p(90.0, 20.0))), "{ends:?}");
        assert!(near(on_arc(c, r, (a0 + a1) / 2.0), p(80.0, 10.0)), "the arc bows away from the mirror line");
        assert_eq!(d.entities[0].layer, "CUT", "layer 0 inherits the INSERT's layer");
    }

    #[test]
    fn unevenly_scaled_insert_turns_a_circle_into_an_ellipse() {
        let circle = "0\nCIRCLE\n8\n0\n10\n5\n20\n10\n40\n3\n";
        let d = dxf(&block("B", circle), "0\nINSERT\n8\nCUT\n2\nB\n10\n0\n20\n0\n41\n2\n42\n1\n");
        let Geom::Ellipse { c, u, v, t0, t1 } = *only(&d) else { panic!("{:?}", d.entities) };
        assert!(near(c, p(10.0, 10.0)) && near(u, p(6.0, 0.0)) && near(v, p(0.0, 3.0)), "{c:?} {u:?} {v:?}");
        assert!(t0.abs() < 1e-12 && (t1 - 2.0 * PI).abs() < 1e-12);
    }

    #[test]
    fn unevenly_scaled_insert_turns_polyline_arcs_into_points_on_the_stretched_arc() {
        // a half circle of radius 5 around (5, 0), as one bulged span and a straight closing span
        let lw = "0
LWPOLYLINE
8
0
90
2
70
1
10
0
20
0
42
1
10
10
20
0
";
        let d = dxf(&block("B", lw), "0
INSERT
8
CUT
2
B
10
0
20
0
41
2
42
1
");
        let Geom::Poly { pts, bulges, closed } = only(&d) else { panic!("{:?}", d.entities) };
        assert!(*closed);
        assert!(pts.len() > 32, "the arc is tessellated: {} points", pts.len());
        assert!(bulges.iter().all(|b| *b == 0.0));
        assert!(near(pts[0], p(0.0, 0.0)));
        for q in pts {
            // on the ellipse with semi-axes 10 (stretched) and 5 around (10, 0), or on the closing line
            let on_ellipse = (((q.x - 10.0) / 10.0).powi(2) + (q.y / 5.0).powi(2) - 1.0).abs() < 1e-9;
            assert!(on_ellipse || q.y.abs() < 1e-9, "{q:?}");
        }
        assert!(pts.iter().any(|q| q.y.abs() > 4.99), "the arc keeps its height");
    }

    #[test]
    fn polyline_seen_from_below_is_mirrored_in_x_with_its_bulges_flipped() {
        let lw = "0\nLWPOLYLINE\n8\nCUT\n90\n2\n70\n0\n10\n10\n20\n0\n42\n0.5\n10\n20\n20\n0\n210\n0\n220\n0\n230\n-1\n";
        let d = dxf("", lw);
        let Geom::Poly { pts, bulges, closed } = only(&d) else { panic!("{:?}", d.entities) };
        assert!(near(pts[0], p(-10.0, 0.0)) && near(pts[1], p(-20.0, 0.0)), "{pts:?}");
        assert_eq!(bulges, &vec![-0.5, 0.0]);
        assert!(!closed);
    }

    #[test]
    fn ellipse_takes_its_minor_axis_from_the_ratio_and_keeps_its_parameters() {
        let el = |t0: f64, t1: f64| format!("0\nELLIPSE\n8\nCUT\n10\n1\n20\n2\n11\n20\n21\n0\n40\n0.5\n41\n{t0}\n42\n{t1}\n");
        let d = dxf("", &el(0.5, 4.0));
        let Geom::Ellipse { c, u, v, t0, t1 } = *only(&d) else { panic!() };
        assert!(near(c, p(1.0, 2.0)) && near(u, p(20.0, 0.0)) && near(v, p(0.0, 10.0)), "{u:?} {v:?}");
        assert_eq!((t0, t1), (0.5, 4.0));
        // an end parameter below the start wraps once around
        let Geom::Ellipse { t0, t1, .. } = *only(&dxf("", &el(5.0, 1.0))) else { panic!() };
        assert_eq!(t0, 5.0);
        assert!((t1 - (1.0 + 2.0 * PI)).abs() < 1e-12);
    }

    #[test]
    fn closed_polyline_keeps_the_bulge_on_its_last_vertex() {
        let lw = "0\nLWPOLYLINE\n8\nCUT\n90\n3\n70\n1\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n10\n42\n0.4\n";
        let d = dxf("", lw);
        let Geom::Poly { pts, bulges, closed } = only(&d) else { panic!() };
        assert_eq!(pts.len(), 3);
        assert_eq!(bulges, &vec![0.0, 0.0, 0.4]);
        assert!(closed);
    }
}
