//! One Clean Up of one source file, as both the window and `dxf-clean` run it.

use crate::clean::{clean, CleanResult, Options};
use crate::dxf_read::{read_file, Drawing};
use crate::dxf_write::{write_dxf, OutputMode};
use crate::geom::BBox;
use crate::spans::{outline, Outline};
use std::path::PathBuf;

/// What the "before" preview shows of the source drawing.
pub struct Before {
    /// One outline per source entity.
    pub outlines: Vec<Outline>,
    /// How many nodes the outlines have in total.
    pub nodes: usize,
    /// The extent of every outline point.
    pub bbox: BBox,
}

impl Default for Before {
    /// Nothing to show: no outlines, no nodes, an empty extent.
    fn default() -> Self {
        Before { outlines: Vec::new(), nodes: 0, bbox: BBox::empty() }
    }
}

/// A source file and the drawing read from it, ready to be cleaned.
pub struct Job {
    source: PathBuf,
    drawing: Drawing,
}

impl Job {
    /// Reads `source`. The error is the reader's own message, ready to show to staff.
    pub fn open(source: PathBuf) -> Result<Job, String> {
        let drawing = read_file(&source)?;
        Ok(Job { source, drawing })
    }

    /// The source's file name, without its folder, as the window shows it.
    pub fn file_name(&self) -> String {
        self.source.file_name().unwrap_or_default().to_string_lossy().into_owned()
    }

    /// The drawing as read: its entities, what was ignored, and its units.
    pub fn drawing(&self) -> &Drawing {
        &self.drawing
    }

    /// Where the cleaned file goes unless staff choose otherwise: `<name>_clean.dxf` beside the source.
    pub fn default_output(&self) -> PathBuf {
        let stem = self.source.file_stem().unwrap_or_default().to_string_lossy();
        self.source.with_file_name(format!("{stem}_clean.dxf"))
    }

    /// Clean Up with `opt`. The job keeps no result: the caller holds on to it.
    pub fn clean(&self, opt: &Options) -> CleanResult {
        clean(&self.drawing, opt)
    }

    /// Traces every source entity for the preview. Not done by `open`: `dxf-clean` never needs it.
    pub fn before(&self) -> Before {
        let outlines: Vec<Outline> = self.drawing.entities.iter().map(|e| outline(&e.geom)).collect();
        let nodes = outlines.iter().map(|o| o.nodes.len()).sum();
        let mut bbox = BBox::empty();
        for q in outlines.iter().flat_map(|o| &o.pts) {
            bbox.add(*q);
        }
        Before { outlines, nodes, bbox }
    }

    /// The cleaned file's contents, in the source drawing's units.
    pub fn dxf_text(&self, res: &CleanResult, mode: OutputMode) -> String {
        write_dxf(&res.shapes, mode, self.drawing.insunits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dxf_read::read_bytes;
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
    }

    #[test]
    fn open_reports_why_the_file_cannot_be_read() {
        let missing = fixture("no_such_file.dxf");
        let err = Job::open(missing.clone()).err().expect("opening a missing file must fail");
        assert!(err.starts_with("Cannot read "), "{err}");
        assert!(err.contains("no_such_file.dxf"), "{err}");
    }

    #[test]
    fn file_name_is_the_source_without_its_folder() {
        let job = Job::open(fixture("illustrator_polluted.dxf")).unwrap();
        assert_eq!(job.file_name(), "illustrator_polluted.dxf");
    }

    #[test]
    fn default_output_sits_beside_the_source_with_clean_appended() {
        let job = Job::open(fixture("illustrator_polluted.dxf")).unwrap();
        assert_eq!(job.default_output(), fixture("illustrator_polluted_clean.dxf"));
    }

    #[test]
    fn clean_turns_the_illustrator_sample_into_two_closed_octagons() {
        let job = Job::open(fixture("illustrator_polluted.dxf")).unwrap();
        let res = job.clean(&Options::default());
        assert_eq!((res.stats.closed_paths, res.stats.open_paths, res.stats.lines), (2, 0, 16), "{:?}", res.stats);
    }

    #[test]
    fn dxf_text_keeps_the_source_units() {
        let job = Job::open(fixture("illustrator_polluted.dxf")).unwrap();
        let res = job.clean(&Options::default());
        let back = read_bytes(job.dxf_text(&res, OutputMode::Polylines).as_bytes()).unwrap();
        assert_eq!(back.insunits, Some(4), "the sample is drawn in mm");
        assert_eq!(back.counts.get("POLYLINE"), Some(&2));
    }

    /// Writes `dxf` to a fresh temporary file named `<name>.dxf` and returns its path.
    fn temp_dxf(name: &str, dxf: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dxf-job-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.dxf"));
        std::fs::write(&path, dxf).unwrap();
        path
    }

    #[test]
    fn before_shows_each_entity_with_its_nodes_and_the_extent_of_the_drawing() {
        let lines_and_circle = "0
SECTION
2
ENTITIES
            0
LINE
8
CUT
10
0
20
0
11
100
21
0
            0
LINE
8
CUT
10
0
20
50
11
100
21
50
            0
CIRCLE
8
CUT
10
150
20
25
40
10
            0
ENDSEC
0
EOF
";
        let job = Job::open(temp_dxf("lines_and_circle", lines_and_circle)).unwrap();
        let before = job.before();
        assert_eq!(before.outlines.len(), 3);
        assert_eq!(before.nodes, 4, "each line has a node at both ends, the circle has none");
        // the circle around (150, 25) with radius 10 reaches past the lines' right end at x = 100
        let bb = before.bbox;
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(near(bb.min.x, 0.0) && near(bb.min.y, 0.0) && near(bb.max.x, 160.0) && near(bb.max.y, 50.0), "{bb:?}");
    }

    #[test]
    fn drawing_reports_what_was_read_and_what_was_ignored() {
        let job = Job::open(fixture("illustrator_polluted.dxf")).unwrap();
        assert_eq!(job.drawing().counts.get("SPLINE"), Some(&2));
        assert_eq!(job.drawing().skipped.get("HATCH"), Some(&2));
    }
}
