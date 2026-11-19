use dxf_converter::*;

fn load(name: &str) -> Drawing {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    read_file(&path).unwrap()
}

fn vertex_counts(res: &CleanResult) -> Vec<usize> {
    let mut v: Vec<usize> = res
        .shapes
        .iter()
        .map(|s| match s {
            Shape::Path(p) => p.segs().len(),
            Shape::Circle { .. } => 0,
        })
        .collect();
    v.sort();
    v
}

#[test]
fn illustrator_sample_matches_manual_fix() {
    let dwg = load("illustrator_polluted.dxf");
    assert_eq!(dwg.counts.get("SPLINE"), Some(&2));
    assert_eq!(dwg.skipped.get("HATCH"), Some(&2));
    let res = clean(&dwg, &Options::default());
    // two closed octagons (square with chamfered corners), 8 straight lines each
    assert_eq!(res.stats.closed_paths, 2, "{:?}", res.stats);
    assert_eq!(res.stats.open_paths, 0);
    assert_eq!(res.stats.arcs, 0);
    assert_eq!(vertex_counts(&res), vec![8, 8]);
    assert!(res.stats.max_deviation < 1e-6);

    // the manual fix has the same topology
    let fix = clean(&load("manual_fix.dxf"), &Options::default());
    assert_eq!(vertex_counts(&fix), vec![8, 8]);
}

#[test]
fn output_roundtrips() {
    let dwg = load("illustrator_polluted.dxf");
    let res = clean(&dwg, &Options::default());
    for mode in [OutputMode::Polylines, OutputMode::LinesAndArcs] {
        let text = write_dxf(&res.shapes, mode, dwg.insunits);
        let back = read_bytes(text.as_bytes()).unwrap();
        let again = clean(&back, &Options::default());
        assert_eq!(vertex_counts(&again), vec![8, 8], "{mode:?}");
        assert!(back.skipped.is_empty());
    }
}

#[test]
fn bulged_polyline_roundtrip() {
    // rounded slot: two lines + two semicircles, written as LINE/ARC, read back and rejoined
    let dxf = "0\nSECTION\n2\nENTITIES\n\
0\nLINE\n8\nCUT\n10\n0\n20\n0\n11\n50\n21\n0\n\
0\nARC\n8\nCUT\n10\n50\n20\n10\n40\n10\n50\n270\n51\n90\n\
0\nLINE\n8\nCUT\n10\n50\n20\n20\n11\n0\n21\n20\n\
0\nARC\n8\nCUT\n10\n0\n20\n10\n40\n10\n50\n90\n51\n270\n\
0\nENDSEC\n0\nEOF\n";
    let dwg = read_bytes(dxf.as_bytes()).unwrap();
    let res = clean(&dwg, &Options::default());
    assert_eq!(res.stats.closed_paths, 1, "{:?}", res.stats);
    assert_eq!(res.stats.lines, 2);
    assert_eq!(res.stats.arcs, 2);
}

#[test]
fn illustrator_rounded_rect_and_hole_become_true_arcs() {
    // Illustrator-style Bezier splines: 200x100 rectangle with R15 corners + R20 hole
    let dwg = load("illustrator_rounded_demo.dxf");
    let res = clean(&dwg, &Options::default());
    let s = &res.stats;
    assert_eq!((s.closed_paths, s.lines, s.arcs, s.circles), (1, 4, 4, 1), "{s:?}");
    for shape in &res.shapes {
        match shape {
            Shape::Circle { r, .. } => assert!((r - 20.0).abs() < 1e-6, "circle r = {r}"),
            Shape::Path(p) => {
                for seg in p.segs().filter(|s| !s.is_line()) {
                    let r = seg.arc().unwrap().r;
                    assert!((r - 15.0).abs() < 1e-6, "corner r = {r}");
                }
            }
        }
    }
}
