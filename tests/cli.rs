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

/// Writes `dxf` to a fresh folder, runs `dxf-clean` on it, returns the output and the
/// path the cleaned file should be written to.
fn run(name: &str, dxf: &str) -> (Output, PathBuf) {
    let dir = std::env::temp_dir().join(format!("dxf-clean-cli-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("in.dxf");
    let output = dir.join("out.dxf");
    std::fs::write(&input, dxf).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dxf-clean")).arg(&input).arg("-o").arg(&output).output().unwrap();
    (out, output)
}

#[test]
fn all_closed_paths_export_quietly() {
    let dxf = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/illustrator_polluted.dxf")).unwrap();
    let (out, written) = run("closed", &dxf);
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
    let (out, written) = run("one", &open_lines(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(written.is_file(), "output file not written");
    assert!(stderr.contains("1 open path:"), "stderr: {stderr:?}");
    assert!(stderr.contains("nesting software will reject"), "stderr: {stderr:?}");
}

#[test]
fn several_open_paths_are_counted_in_the_plural() {
    let (out, written) = run("two", &open_lines(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(written.is_file(), "output file not written");
    assert!(stderr.contains("2 open paths:"), "stderr: {stderr:?}");
    assert!(stderr.contains("unless they are marking lines"), "stderr: {stderr:?}");
}
