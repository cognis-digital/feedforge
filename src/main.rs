//! feedforge CLI — build, validate, and merge RSS/Atom feeds.
//!
//! Argument parsing is hand-rolled (no clap dependency) to keep the binary
//! lean for use inside CI containers and monitoring stacks.

use std::process::ExitCode;

use feedforge::build::{build_feed, feed_from_json};
use feedforge::merge::merge_files;
use feedforge::model::FeedFormat;
use feedforge::validate::validate_feed;
use feedforge::Error;

const USAGE: &str = "\
feedforge — RSS/Atom feed builder, validator, and merger

USAGE:
    feedforge build <items.json> [--format rss|atom] [-o <out.xml>]
    feedforge validate <feed.xml>
    feedforge merge <feed1.xml> <feed2.xml> ... [--format rss|atom] [-o <out.xml>]
    feedforge --help | --version

COMMANDS:
    build       Render a feed from a JSON items file.
    validate    Check a feed for well-formedness and required fields (CI gate).
    merge       Merge feeds, de-duplicate by link/guid, sort newest-first.

OPTIONS:
    --format    Output format: rss (default) or atom.
    -o <file>   Write output to <file> instead of stdout.
    -h, --help  Show this help.
    --version   Show version.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, Error> {
    let Some(cmd) = args.first() else {
        eprint!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };

    match cmd.as_str() {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        "--version" | "-V" => {
            println!("feedforge {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        "build" => cmd_build(&args[1..]),
        "validate" => cmd_validate(&args[1..]),
        "merge" => cmd_merge(&args[1..]),
        other => {
            eprintln!("error: unknown command '{other}'\n");
            eprint!("{USAGE}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Parsed common options: positional args, format, and output path.
struct Opts {
    positional: Vec<String>,
    format: FeedFormat,
    out: Option<String>,
}

fn parse_opts(args: &[String]) -> Result<Opts, Error> {
    let mut positional = Vec::new();
    let mut format = FeedFormat::Rss;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" | "-f" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| Error::Invalid("--format needs a value".into()))?;
                format = FeedFormat::parse(v)
                    .ok_or_else(|| Error::Invalid(format!("unknown format '{v}'")))?;
                i += 2;
            }
            "-o" | "--output" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| Error::Invalid("-o needs a value".into()))?;
                out = Some(v.clone());
                i += 2;
            }
            other => {
                positional.push(other.to_string());
                i += 1;
            }
        }
    }
    Ok(Opts {
        positional,
        format,
        out,
    })
}

fn emit(content: &str, out: &Option<String>) -> Result<(), Error> {
    match out {
        Some(path) => {
            std::fs::write(path, content)?;
            eprintln!("wrote {} bytes to {path}", content.len());
        }
        None => print!("{content}"),
    }
    Ok(())
}

fn cmd_build(args: &[String]) -> Result<ExitCode, Error> {
    let opts = parse_opts(args)?;
    let input = opts
        .positional
        .first()
        .ok_or_else(|| Error::Invalid("build needs an <items.json> path".into()))?;
    let json = std::fs::read_to_string(input)?;
    let feed = feed_from_json(&json)?;
    let xml = build_feed(&feed, opts.format)?;
    emit(&xml, &opts.out)?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_validate(args: &[String]) -> Result<ExitCode, Error> {
    let opts = parse_opts(args)?;
    let path = opts
        .positional
        .first()
        .ok_or_else(|| Error::Invalid("validate needs a <feed.xml> path".into()))?;
    match validate_feed(path) {
        Ok(report) => {
            println!(
                "OK: {} feed, {} item(s) valid",
                report.format, report.item_count
            );
            for w in &report.warnings {
                println!("warning: {w}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            // Validation/XML failures must fail the process for CI gating.
            eprintln!("INVALID: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn cmd_merge(args: &[String]) -> Result<ExitCode, Error> {
    let opts = parse_opts(args)?;
    if opts.positional.len() < 2 {
        return Err(Error::Invalid("merge needs at least two feed paths".into()));
    }
    let xml = merge_files(&opts.positional, opts.format)?;
    emit(&xml, &opts.out)?;
    Ok(ExitCode::SUCCESS)
}
