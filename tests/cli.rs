//! `dxf-clean` as staff and scripts run it. A warning goes to stderr and the file is still
//! written; a mistake in the arguments goes to stderr, writes nothing and exits with code 2.

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
    let layers: Vec<String> = dxf_cleaner::read_file(&written).unwrap().entities.into_iter().map(|e| e.layer).collect();
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

#[test]
fn without_o_the_clean_file_lands_beside_the_input_and_the_input_is_untouched() {
    let dir = std::env::temp_dir().join(format!("dxf-clean-cli-test-default-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("part.dxf");
    let dxf = open_lines(1);
    std::fs::write(&input, &dxf).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dxf-clean")).arg(&input).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join("part_clean.dxf").is_file(), "part_clean.dxf not written");
    assert_eq!(std::fs::read_to_string(&input).unwrap(), dxf, "the input was changed");
}

/// Asserts `dxf-clean` stopped on an argument mistake: exit 2, nothing on stdout, no file
/// written, and stderr is exactly `error_line` followed by the usage line.
fn assert_usage_error(out: &Output, written: &PathBuf, error_line: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "stderr: {stderr}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "", "stderr: {stderr}");
    assert!(!written.exists(), "a file was written despite the error");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 2, "stderr: {stderr:?}");
    assert_eq!(lines[0], error_line);
    assert!(lines[1].starts_with("usage: dxf-clean <input.dxf> "), "stderr: {stderr:?}");
}

#[test]
fn no_input_file_is_reported() {
    let dir = std::env::temp_dir().join(format!("dxf-clean-cli-test-no-input-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for flags in [&[][..], &["--origin", "-o", "out.dxf"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_dxf-clean")).args(flags).current_dir(&dir).output().unwrap();
        assert_usage_error(&out, &dir.join("out.dxf"), "error: no input file");
    }
}

#[test]
fn a_mistyped_flag_is_reported_not_taken_as_the_input() {
    let (out, written) = run("typo", &open_lines(1), &["--layer-0"]);
    assert_usage_error(&out, &written, "error: unknown argument '--layer-0'");
    let (out, written) = run("typo-short", &open_lines(1), &["-x"]);
    assert_usage_error(&out, &written, "error: unknown argument '-x'");
}

#[test]
fn a_second_input_path_is_reported_not_swapped_in() {
    let (out, written) = run("two-inputs", &open_lines(1), &["other.dxf"]);
    let input = written.with_file_name("in.dxf");
    assert_usage_error(&out, &written, &format!("error: more than one input file: '{}' and 'other.dxf'", input.display()));
}

#[test]
fn a_tolerance_that_is_not_a_number_is_reported() {
    let (out, written) = run("tol-abc", &open_lines(1), &["--tol", "abc"]);
    assert_usage_error(&out, &written, "error: --tol: 'abc' is not a number");
    let (out, written) = run("tol-nan", &open_lines(1), &["--tol", "nan"]);
    assert_usage_error(&out, &written, "error: --tol: 'nan' is not a number");
}

#[test]
fn a_tolerance_with_no_value_is_reported() {
    let (out, written) = run("tol-missing", &open_lines(1), &["--tol"]);
    assert_usage_error(&out, &written, "error: --tol needs a number");
    let (out, written) = run("tol-then-flag", &open_lines(1), &["--tol", "--layer0"]);
    assert_usage_error(&out, &written, "error: --tol needs a number");
}

#[test]
fn a_tolerance_of_zero_or_less_is_reported() {
    let (out, written) = run("tol-zero", &open_lines(1), &["--tol", "0"]);
    assert_usage_error(&out, &written, "error: --tol must be greater than 0, got '0'");
    let (out, written) = run("tol-negative", &open_lines(1), &["--tol", "-0.05"]);
    assert_usage_error(&out, &written, "error: --tol must be greater than 0, got '-0.05'");
}

#[test]
fn a_join_gap_that_is_missing_or_not_a_number_is_reported() {
    let (out, written) = run("join-missing", &open_lines(1), &["--join"]);
    assert_usage_error(&out, &written, "error: --join needs a number");
    let (out, written) = run("join-abc", &open_lines(1), &["--join", "abc"]);
    assert_usage_error(&out, &written, "error: --join: 'abc' is not a number");
    let (out, written) = run("join-inf", &open_lines(1), &["--join", "inf"]);
    assert_usage_error(&out, &written, "error: --join: 'inf' is not a number");
}

#[test]
fn a_negative_join_gap_is_reported_but_zero_is_allowed() {
    let (out, written) = run("join-negative", &open_lines(1), &["--join", "-1"]);
    assert_usage_error(&out, &written, "error: --join must be 0 or more, got '-1'");
    let (out, written) = run("join-zero", &open_lines(1), &["--join", "0"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(written.is_file(), "output file not written");
}

#[test]
fn an_output_flag_with_no_file_name_is_reported() {
    let (out, written) = run("o-missing", &open_lines(1), &["-o"]);
    assert_usage_error(&out, &written, "error: -o needs a file name");
    let (out, written) = run("output-then-flag", &open_lines(1), &["--output", "--layer0"]);
    assert_usage_error(&out, &written, "error: --output needs a file name");
}

/// Two concentric circles of radius 0.25 and 0.28, with `header` before the entities.
fn concentric_circles(header: &str) -> String {
    format!("{header}0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\nCUT\n10\n0\n20\n0\n40\n0.25\n0\nCIRCLE\n8\nCUT\n10\n0\n20\n0\n40\n0.28\n0\nENDSEC\n0\nEOF\n")
}

const INCH_HEADER: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n";

#[test]
fn the_tolerance_is_mm_in_an_inch_drawing() {
    // 0.03 in apart: at a 0.05 mm Tolerance these are two shapes, not a duplicate
    let (out, written) = run("inch", &concentric_circles(INCH_HEADER), &[]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(summary(&out).contains("2 circles, 0 duplicates removed"), "{}", summary(&out));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "", "an inch drawing names its units");
    assert_eq!(dxf_cleaner::read_file(&written).unwrap().insunits, Some(1), "the cleaned file stays in inches");
}

#[test]
fn a_drawing_with_no_units_warns_it_was_treated_as_mm_but_still_exports() {
    let (out, written) = run("no-units", &concentric_circles(""), &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {stderr}");
    assert!(written.is_file(), "output file not written");
    assert!(stderr.lines().any(|l| l == "warning: no units in drawing: treated as mm"), "stderr: {stderr:?}");
}

#[test]
fn the_usage_says_the_tolerance_and_join_gap_are_mm() {
    let out = Command::new(env!("CARGO_BIN_EXE_dxf-clean")).arg("--help").output().unwrap();
    let usage = String::from_utf8_lossy(&out.stdout);
    assert!(usage.contains("[--tol <mm, default 0.05>]") && usage.contains("[--join <mm, default 0.05>]"), "{usage}");
}
