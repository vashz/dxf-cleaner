//! `dxf-clean` as staff and scripts run it: warnings on stderr, the file still written.

use std::path::PathBuf;
use std::process::{Command, Output};

/// A drawing of `n` separate straight lines, far apart: each cleans to one open path.
fn open_lines(n: usize) -> String {
    let mut dxf = String::from("0\nSECTION\n2\nENTITIES\n");
    for i in 0..n {
        let y = i as f64 * 50.0;
        dxf += &format!("0\nLINE\n8\nCUT\n10\n0\n20\n{y}\n11\n100\n21\n{y}\n");
    }
    dxf + "0\nENDSEC\n0\nEOF\n"
}

/// Writes `dxf` to a fresh folder, runs `dxf-clean` on it with `flags`, returns the output
/// and the path the cleaned file should be written to.
fn run(name: &str, dxf: &str, flags: &[&str]) -> (Output, PathBuf) {
    let dir = std::env::temp_dir().join(format!("dxf-clean-cli-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.dxf");
    let output = dir.join("out.dxf");
    std::fs::write(&input, dxf).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dxf-clean")).arg(&input).arg("-o").arg(&output).args(flags).output().unwrap();
    (out, output)
}

#[test]
fn all_closed_paths_export_quietly() {
    let dxf = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/illustrator_polluted.dxf")).unwrap();
    let (out, written) = run("closed", &dxf, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0));
    assert!(written.is_file(), "output file not written");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    // summary line as printed before this change (dxf-clean 0.1.1)
    let summary = stdout.lines().find(|l| l.starts_with("output:")).unwrap_or_default();
    assert_eq!(summary, "output: 2 closed + 0 open paths, 16 lines, 0 arcs, 0 circles, 0 duplicates removed, max deviation 0.0000");
}

#[test]
fn one_open_path_warns_on_stderr_but_still_exports() {
    let (out, written) = run("one", &open_lines(1), &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(written.is_file(), "output file not written");
    assert!(stderr.contains("1 open path:"), "stderr: {stderr:?}");
    assert!(stderr.contains("nesting software will reject"), "stderr: {stderr:?}");
}

#[test]
fn several_open_paths_are_counted_in_the_plural() {
    let (out, written) = run("two", &open_lines(2), &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(written.is_file(), "output file not written");
    assert!(stderr.contains("2 open paths:"), "stderr: {stderr:?}");
    assert!(stderr.contains("unless they are marking lines"), "stderr: {stderr:?}");
}

/// The `output:` summary line.
fn summary(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).lines().find(|l| l.starts_with("output:")).unwrap_or_default().to_string()
}

#[test]
fn layer0_puts_every_entity_on_layer_0() {
    let (out, written) = run("layer0", &open_lines(2), &["--layer0"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let layers: Vec<String> = dxf_converter::read_file(&written).unwrap().entities.into_iter().map(|e| e.layer).collect();
    assert_eq!(layers, ["0", "0"], "the source lines are on layer CUT");
}

#[test]
fn no_join_leaves_touching_lines_as_separate_paths() {
    // two lines meeting at (100, 0)
    let corner = "0
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
100
20
0
11
100
21
50
        0
ENDSEC
0
EOF
";
    let (joined, _) = run("join", corner, &[]);
    assert!(summary(&joined).starts_with("output: 0 closed + 1 open paths"), "{}", summary(&joined));
    let (apart, written) = run("no-join", corner, &["--no-join"]);
    assert!(written.is_file(), "output file not written");
    assert!(summary(&apart).starts_with("output: 0 closed + 2 open paths"), "{}", summary(&apart));
}
