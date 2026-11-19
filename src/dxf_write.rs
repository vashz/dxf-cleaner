//! DXF writer. Emits a small, widely compatible R12 (AC1009) file containing
//! only POLYLINE (with arc bulges), LINE, ARC and CIRCLE entities.

use crate::clean::Shape;
use crate::geom::BBox;
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    /// One continuous POLYLINE per path (lines + arcs via bulge). Best for cutting.
    Polylines,
    /// Separate LINE / ARC entities.
    LinesAndArcs,
}

fn num(v: f64) -> String {
    let v = if v.abs() < 1e-12 { 0.0 } else { v };
    let mut s = format!("{v:.10}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.push('0');
    }
    s
}

struct W(String);

impl W {
    fn pair(&mut self, code: i32, value: &str) {
        let _ = write!(self.0, "{code:>3}\r\n{value}\r\n");
    }
    fn f(&mut self, code: i32, v: f64) {
        self.pair(code, &num(v));
    }
    fn i(&mut self, code: i32, v: i64) {
        self.pair(code, &format!("{v:>6}"));
    }
}

fn clean_layer(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "_-$".contains(c) { c } else { '_' })
        .collect();
    if s.is_empty() { "0".into() } else { s }
}

pub fn write_dxf(shapes: &[Shape], mode: OutputMode, insunits: Option<i32>) -> String {
    let mut w = W(String::new());
    let mut bb = BBox::empty();
    for s in shapes {
        bb.union(&s.bbox());
    }
    if !bb.is_valid() {
        bb = BBox { min: crate::geom::p(0.0, 0.0), max: crate::geom::p(0.0, 0.0) };
    }
    let layers: BTreeSet<String> = std::iter::once("0".to_string())
        .chain(shapes.iter().map(|s| clean_layer(s.layer())))
        .collect();

    // HEADER
    w.pair(0, "SECTION");
    w.pair(2, "HEADER");
    w.pair(9, "$ACADVER");
    w.pair(1, "AC1009");
    if let Some(u) = insunits {
        w.pair(9, "$INSUNITS");
        w.i(70, u as i64);
    }
    w.pair(9, "$EXTMIN");
    w.f(10, bb.min.x);
    w.f(20, bb.min.y);
    w.f(30, 0.0);
    w.pair(9, "$EXTMAX");
    w.f(10, bb.max.x);
    w.f(20, bb.max.y);
    w.f(30, 0.0);
    w.pair(0, "ENDSEC");

    // TABLES
    w.pair(0, "SECTION");
    w.pair(2, "TABLES");
    w.pair(0, "TABLE");
    w.pair(2, "LTYPE");
    w.i(70, 1);
    w.pair(0, "LTYPE");
    w.pair(2, "CONTINUOUS");
    w.i(70, 0);
    w.pair(3, "Solid line");
    w.i(72, 65);
    w.i(73, 0);
    w.f(40, 0.0);
    w.pair(0, "ENDTAB");
    w.pair(0, "TABLE");
    w.pair(2, "LAYER");
    w.i(70, layers.len() as i64);
    for l in &layers {
        w.pair(0, "LAYER");
        w.pair(2, l);
        w.i(70, 0);
        w.i(62, 7);
        w.pair(6, "CONTINUOUS");
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "ENDSEC");

    // BLOCKS (empty, some readers expect the section)
    w.pair(0, "SECTION");
    w.pair(2, "BLOCKS");
    w.pair(0, "ENDSEC");

    // ENTITIES
    w.pair(0, "SECTION");
    w.pair(2, "ENTITIES");
    for s in shapes {
        let layer = clean_layer(s.layer());
        match s {
            Shape::Circle { c, r, .. } => {
                w.pair(0, "CIRCLE");
                w.pair(8, &layer);
                w.f(10, c.x);
                w.f(20, c.y);
                w.f(30, 0.0);
                w.f(40, *r);
            }
            Shape::Path(path) => match mode {
                OutputMode::Polylines => {
                    w.pair(0, "POLYLINE");
                    w.pair(8, &layer);
                    w.i(66, 1);
                    w.f(10, 0.0);
                    w.f(20, 0.0);
                    w.f(30, 0.0);
                    w.i(70, if path.closed { 1 } else { 0 });
                    let mut vertex = |pt: crate::geom::P, bulge: f64| {
                        w.pair(0, "VERTEX");
                        w.pair(8, &layer);
                        w.f(10, pt.x);
                        w.f(20, pt.y);
                        w.f(30, 0.0);
                        if bulge.abs() > crate::geom::LINE_BULGE_EPS {
                            w.f(42, bulge);
                        }
                    };
                    for seg in &path.segs {
                        vertex(seg.a, seg.bulge);
                    }
                    if !path.closed {
                        vertex(path.end(), 0.0);
                    }
                    w.pair(0, "SEQEND");
                    w.pair(8, &layer);
                }
                OutputMode::LinesAndArcs => {
                    for seg in &path.segs {
                        match seg.arc() {
                            None => {
                                w.pair(0, "LINE");
                                w.pair(8, &layer);
                                w.f(10, seg.a.x);
                                w.f(20, seg.a.y);
                                w.f(30, 0.0);
                                w.f(11, seg.b.x);
                                w.f(21, seg.b.y);
                                w.f(31, 0.0);
                            }
                            Some(ai) => {
                                // DXF arcs are CCW from start to end angle
                                let (sa, ea) = if ai.sweep >= 0.0 {
                                    ((seg.a - ai.c).angle(), (seg.b - ai.c).angle())
                                } else {
                                    ((seg.b - ai.c).angle(), (seg.a - ai.c).angle())
                                };
                                w.pair(0, "ARC");
                                w.pair(8, &layer);
                                w.f(10, ai.c.x);
                                w.f(20, ai.c.y);
                                w.f(30, 0.0);
                                w.f(40, ai.r);
                                w.f(50, sa.to_degrees().rem_euclid(360.0));
                                w.f(51, ea.to_degrees().rem_euclid(360.0));
                            }
                        }
                    }
                }
            },
        }
    }
    w.pair(0, "ENDSEC");
    w.pair(0, "EOF");
    w.0
}
