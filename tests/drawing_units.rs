//! The Tolerance and Join gap are mm whatever units the drawing is in (ADR 0001): Clean Up
//! converts them to drawing units from `$INSUNITS`, and the cleaned file keeps the source units.

use dxf_cleaner::dxf_read::{Entity, Geom};
use dxf_cleaner::geom::p;
use dxf_cleaner::*;

const INCH: i32 = 1;

fn ent(geom: Geom) -> Entity {
    Entity { layer: "0".into(), kind: "TEST", geom }
}

fn drawing(insunits: Option<i32>, geoms: Vec<Geom>) -> Drawing {
    Drawing { entities: geoms.into_iter().map(ent).collect(), insunits, ..Default::default() }
}

#[test]
fn concentric_circles_in_an_inch_drawing_are_not_duplicates() {
    // 0.03 in (0.76 mm) apart: distinct shapes at a 0.05 mm Tolerance
    let d = drawing(Some(INCH), vec![Geom::Circle { c: p(0.0, 0.0), r: 0.25 }, Geom::Circle { c: p(0.0, 0.0), r: 0.28 }]);
    let res = clean(&d, &Options::default());
    assert_eq!((res.stats.circles, res.stats.duplicates_removed), (2, 0), "{:?}", res.stats);
}

const MM: i32 = 4;
const METRE: i32 = 6;
const KM: i32 = 7;

fn load(name: &str) -> Drawing {
    read_file(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// `d` drawn `k` times larger, in `insunits`.
fn scaled(d: &Drawing, k: f64, insunits: Option<i32>) -> Drawing {
    let s = |q: &dxf_cleaner::geom::P| *q * k;
    let entities = d
        .entities
        .iter()
        .map(|e| {
            let geom = match &e.geom {
                Geom::Line { a, b } => Geom::Line { a: s(a), b: s(b) },
                Geom::Arc { c, r, a0, a1 } => Geom::Arc { c: s(c), r: r * k, a0: *a0, a1: *a1 },
                Geom::Circle { c, r } => Geom::Circle { c: s(c), r: r * k },
                Geom::Ellipse { c, u, v, t0, t1 } => Geom::Ellipse { c: s(c), u: s(u), v: s(v), t0: *t0, t1: *t1 },
                Geom::Poly { pts, bulges, closed } => Geom::Poly { pts: pts.iter().map(s).collect(), bulges: bulges.clone(), closed: *closed },
                Geom::Spline { degree, knots, ctrl, weights, fit, closed } => Geom::Spline {
                    degree: *degree,
                    knots: knots.clone(),
                    ctrl: ctrl.iter().map(s).collect(),
                    weights: weights.clone(),
                    fit: fit.iter().map(s).collect(),
                    closed: *closed,
                },
            };
            Entity { geom, ..e.clone() }
        })
        .collect();
    Drawing { entities, insunits, ..d.clone() }
}

/// An S-shaped Bezier 100 mm across, drawn in mm: arcs only approximate it.
fn s_curve_mm() -> Drawing {
    let ctrl = vec![p(0.0, 0.0), p(100.0, 0.0), p(0.0, 100.0), p(100.0, 100.0)];
    let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
    drawing(Some(MM), vec![Geom::Spline { degree: 3, knots, weights: vec![1.0; 4], ctrl, fit: vec![], closed: false }])
}

/// What staff check before cutting: segment counts and closed/open paths.
fn counts(res: &CleanResult) -> (usize, usize, usize, usize, usize) {
    let s = &res.stats;
    (s.lines, s.arcs, s.circles, s.closed_paths, s.open_paths)
}

#[test]
fn the_same_outline_in_mm_and_in_inches_cleans_the_same() {
    let mut mm = load("illustrator_rounded_demo.dxf");
    mm.entities.extend(s_curve_mm().entities);
    assert_eq!(mm.insunits, Some(MM));
    let inches = scaled(&mm, 1.0 / 25.4, Some(INCH));
    let (a, b) = (clean(&mm, &Options::default()), clean(&inches, &Options::default()));
    assert_eq!(counts(&a), counts(&b), "mm {:?}\ninches {:?}", a.stats, b.stats);
}

#[test]
fn max_deviation_is_reported_in_mm() {
    let mm = s_curve_mm();
    let inches = scaled(&mm, 1.0 / 25.4, Some(INCH));
    let (a, b) = (clean(&mm, &Options::default()), clean(&inches, &Options::default()));
    assert!(a.stats.max_deviation > 0.01 && a.stats.max_deviation <= 0.05 + 1e-9, "{:?}", a.stats);
    assert!((a.stats.max_deviation - b.stats.max_deviation).abs() < 1e-6, "mm {:?}\ninches {:?}", a.stats, b.stats);
}

#[test]
fn a_metre_drawing_does_not_join_ends_10_mm_apart() {
    let d = drawing(Some(METRE), vec![Geom::Line { a: p(0.0, 0.0), b: p(1.0, 0.0) }, Geom::Line { a: p(1.01, 0.0), b: p(2.0, 0.0) }]);
    let res = clean(&d, &Options::default());
    assert_eq!(res.stats.open_paths, 2, "the 0.05 mm Join gap must not reach 0.01 m: {:?}", res.stats);
}

#[test]
fn a_km_drawing_keeps_the_005_mm_tolerance() {
    // Raising the Tolerance to the 1e-6 minimum after converting would make it 1 mm (1e-6 km).
    let mm = s_curve_mm();
    let km = scaled(&mm, 1e-6, Some(KM));
    let (a, b) = (clean(&mm, &Options::default()), clean(&km, &Options::default()));
    assert!(b.stats.max_deviation <= 0.05 + 1e-9, "{:?}", b.stats);
    assert_eq!(counts(&a), counts(&b), "mm {:?}\nkm {:?}", a.stats, b.stats);
}

#[test]
fn a_drawing_with_no_units_cleans_as_mm() {
    for name in ["illustrator_polluted.dxf", "illustrator_rounded_demo.dxf", "manual_fix.dxf"] {
        let mm = load(name);
        assert_eq!(mm.insunits, Some(MM), "{name}");
        for unitless in [None, Some(0)] {
            let none = Drawing { insunits: unitless, ..mm.clone() };
            let (a, b) = (clean(&mm, &Options::default()), clean(&none, &Options::default()));
            assert_eq!(format!("{:?}", a.shapes), format!("{:?}", b.shapes), "{name}, $INSUNITS {unitless:?}");
            assert_eq!(format!("{:?}", a.stats), format!("{:?}", b.stats), "{name}, $INSUNITS {unitless:?}");
        }
    }
}

#[test]
fn the_cleaned_file_keeps_the_source_coordinates_and_units() {
    let d = drawing(Some(INCH), vec![Geom::Circle { c: p(1.0, 2.0), r: 0.25 }, Geom::Circle { c: p(1.0, 2.0), r: 0.28 }]);
    let res = clean(&d, &Options::default());
    for mode in [OutputMode::Polylines, OutputMode::LinesAndArcs] {
        let back = read_bytes(write_dxf(&res.shapes, mode, d.insunits).as_bytes()).unwrap();
        assert_eq!(back.insunits, Some(INCH), "{mode:?}");
        let mut radii: Vec<f64> = back
            .entities
            .iter()
            .map(|e| match e.geom {
                Geom::Circle { c, r } => {
                    assert!(c.dist(p(1.0, 2.0)) < 1e-9, "{c:?}");
                    r
                }
                ref g => panic!("expected circles, got {g:?}"),
            })
            .collect();
        radii.sort_by(f64::total_cmp);
        assert_eq!(radii, [0.25, 0.28], "{mode:?}");
    }
}
