//! Command-line version of DXF Cleaner. The flags are listed once, in `USAGE`.

use dxf_cleaner::*;
use std::path::PathBuf;

const USAGE: &str = "dxf-clean <input.dxf> [-o output.dxf] [--tol <mm, default 0.05>] [--join <mm, default 0.05>] [--no-join] [--lines-arcs] [--origin] [--keep-duplicates] [--layer0]";

/// What the command line asks for.
struct Cli {
    input: PathBuf,
    output: Option<PathBuf>,
    opt: Options,
    mode: OutputMode,
}

/// Reads the arguments, or names the first one that is wrong. `Ok(None)` means `--help`.
fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Cli>, String> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut opt = Options::default();
    let mut mode = OutputMode::Polylines;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(value(&a, args.next(), "a file name")?)),
            "--tol" => {
                let (n, v) = number(&a, args.next())?;
                if n <= 0.0 {
                    return Err(format!("--tol must be greater than 0, got '{v}'"));
                }
                opt.tolerance = n;
            }
            "--join" => {
                let (n, v) = number(&a, args.next())?;
                if n < 0.0 {
                    return Err(format!("--join must be 0 or more, got '{v}'"));
                }
                opt.join_gap = n;
            }
            "--no-join" => opt.join_paths = false,
            "--lines-arcs" => mode = OutputMode::LinesAndArcs,
            "--origin" => opt.move_to_origin = true,
            "--keep-duplicates" => opt.remove_duplicates = false,
            "--layer0" => opt.put_everything_on_layer_0(true),
            "-h" | "--help" => return Ok(None),
            flag if flag.starts_with('-') => return Err(format!("unknown argument '{flag}'")),
            _ => match &input {
                Some(first) => return Err(format!("more than one input file: '{}' and '{a}'", first.display())),
                None => input = Some(PathBuf::from(a)),
            },
        }
    }
    let input = input.ok_or("no input file")?;
    Ok(Some(Cli { input, output, opt, mode }))
}

/// The value after `flag`. Nothing there, or another `--` flag in its place, is a mistake.
fn value(flag: &str, next: Option<String>, what: &str) -> Result<String, String> {
    next.filter(|v| !v.starts_with("--")).ok_or_else(|| format!("{flag} needs {what}"))
}

/// The finite number after `flag`, with the text it was read from.
fn number(flag: &str, next: Option<String>) -> Result<(f64, String), String> {
    let v = value(flag, next, "a number")?;
    match v.parse::<f64>() {
        Ok(n) if n.is_finite() => Ok((n, v)),
        _ => Err(format!("{flag}: '{v}' is not a number")),
    }
}

fn main() {
    let Cli { input, output, opt, mode } = match parse(std::env::args().skip(1)) {
        Ok(Some(cli)) => cli,
        Ok(None) => {
            println!("{USAGE}");
            return;
        }
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("usage: {USAGE}");
            std::process::exit(2);
        }
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
    // The max deviation is in mm, like --tol. The line reads as before, for scripts that parse it.
    println!(
        "output: {} closed + {} open paths, {} lines, {} arcs, {} circles, {} duplicates removed, max deviation {:.4}",
        s.closed_paths, s.open_paths, s.lines, s.arcs, s.circles, s.duplicates_removed, s.max_deviation
    );
    println!("wrote {}", out.display());
    if let Some(note) = dwg.units().note() {
        // --tol and --join are mm, so staff must know when the drawing's own units were guessed.
        eprintln!("warning: {}{}", note[..1].to_lowercase(), &note[1..]);
    }
    if let Some(w) = s.open_path_warning() {
        eprintln!("warning: {w}");
    }
}
