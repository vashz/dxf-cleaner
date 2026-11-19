//! Command-line version of DXF Cleaner. The flags are listed once, in `USAGE`.

use dxf_cleaner::*;
use std::path::PathBuf;

const USAGE: &str = "dxf-clean <input.dxf> [-o output.dxf] [--tol 0.05] [--join 0.05] [--no-join] [--lines-arcs] [--origin] [--keep-duplicates] [--layer0]";

fn main() {
    let mut args = std::env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut opt = Options::default();
    let mut mode = OutputMode::Polylines;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => output = args.next().map(PathBuf::from),
            "--tol" => opt.tolerance = args.next().and_then(|v| v.parse().ok()).unwrap_or(opt.tolerance),
            "--join" => opt.join_gap = args.next().and_then(|v| v.parse().ok()).unwrap_or(opt.join_gap),
            "--no-join" => opt.join_paths = false,
            "--lines-arcs" => mode = OutputMode::LinesAndArcs,
            "--origin" => opt.move_to_origin = true,
            "--keep-duplicates" => opt.remove_duplicates = false,
            "--layer0" => opt.put_everything_on_layer_0(true),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => input = Some(PathBuf::from(a)),
        }
    }
    let Some(input) = input else {
        eprintln!("usage: {USAGE}");
        std::process::exit(2);
    };
    let job = match Job::open(input) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let res = job.clean(&opt);
    let out = output.unwrap_or_else(|| job.default_output());
    if let Err(e) = std::fs::write(&out, job.dxf_text(&res, mode)) {
        eprintln!("error writing {}: {e}", out.display());
        std::process::exit(1);
    }
    let dwg = job.drawing();
    println!("input : {:?}", dwg.counts);
    if !dwg.skipped.is_empty() {
        println!("ignored: {:?}", dwg.skipped);
    }
    let s = &res.stats;
    println!(
        "output: {} closed + {} open paths, {} lines, {} arcs, {} circles, {} duplicates removed, max deviation {:.4}",
        s.closed_paths, s.open_paths, s.lines, s.arcs, s.circles, s.duplicates_removed, s.max_deviation
    );
    println!("wrote {}", out.display());
    if let Some(w) = s.open_path_warning() {
        eprintln!("warning: {w}");
    }
}
