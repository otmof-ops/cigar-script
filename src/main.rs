// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar`: the CigarScript command line. `cigar check` reports on a file
//! (or a folder's `.cig` and `.cigar` files at the top level) the way `cig
//! check` does; `cigar run` checks a file and runs it.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use cigarscript::cli::{ctx_from, exit, run_check, Flags};
use cigarscript::features::Strength;

#[derive(Parser)]
#[command(
    name = "cigar",
    about = "CigarScript: CigScript grown deeper, in three strengths",
    disable_version_flag = true
)]
struct Cli {
    /// Machine-readable output on stdout.
    #[arg(long, global = true)]
    json: bool,
    /// The same information without the catchphrases (also CIG_PLAIN).
    #[arg(long, global = true)]
    plain: bool,
    /// Never colour (also NO_COLOR).
    #[arg(long, global = true)]
    no_color: bool,
    /// Print the version.
    #[arg(long, short = 'V')]
    version: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Check a file, or every .cig and .cigar file at the top of a folder.
    Check(CheckArgs),
    /// Check a file, then run it: every burn journaled, rolled back if it fails.
    Run(RunArgs),
    /// The checks only cigar can make, for `ciginstaller doctor --tools`.
    #[command(name = "__doctor", hide = true)]
    Doctor,
}

#[derive(Args)]
struct RunArgs {
    /// The file to run.
    file: PathBuf,
    /// Simulate every burn and print the plan instead of executing it.
    #[arg(long)]
    dry_run: bool,
    /// Leave burns in place if the script fails.
    #[arg(long)]
    no_rollback: bool,
    /// Step budget before "your loop never ends" (E515); 0 disables it.
    #[arg(long, value_name = "N")]
    max_steps: Option<u64>,
    /// Check as if the file declared this strength.
    #[arg(long, value_name = "mild|medium|full")]
    strength: Option<String>,
    /// Arguments passed to the script as `args`.
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Args)]
struct CheckArgs {
    /// The files or folders to check.
    paths: Vec<PathBuf>,
    /// Check as if the file declared this strength (turning down is
    /// checked). Ignored for .cig files, which are always mild.
    #[arg(long, value_name = "mild|medium|full")]
    strength: Option<String>,
}

fn main() {
    // A program `cigar run` built is cigar with a script appended: it runs
    // as cig runs a program.
    if cigarscript::run::carries_program() {
        std::process::exit(cigscript::cli::entry());
    }
    std::process::exit(run());
}

fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() {
                exit::USAGE
            } else {
                exit::OK
            };
            let _ = e.print();
            return code;
        }
    };
    if cli.version {
        println!("cigar {}", env!("CARGO_PKG_VERSION"));
        return exit::OK;
    }
    match &cli.command {
        None => {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            exit::OK
        }
        Some(Command::Check(a)) => check_cmd(&cli, a),
        Some(Command::Run(a)) => run_cmd(&cli, a),
        Some(Command::Doctor) => cigarscript::doctor::run(cli.json),
    }
}

fn run_cmd(top: &Cli, a: &RunArgs) -> i32 {
    let flags = Flags {
        paths: vec![a.file.clone()],
        strength: None,
        json: top.json,
        plain: top.plain,
        no_color: top.no_color,
    };
    let ctx = ctx_from(&flags);
    let strength = match a.strength.as_deref() {
        None => None,
        Some(s) => match Strength::parse(s) {
            Some(st) => Some(st),
            None => {
                let d = cigscript::diagnostics::usage(format!(
                    "--strength is `{s}`, want `mild`, `medium` or `full`"
                ))
                .code("E800")
                .with_hint("cigar run --strength mild, medium or full");
                cigscript::cli::report(&ctx, &d, None, None);
                return exit::USAGE;
            }
        },
    };
    let rf = cigarscript::run::RunFlags {
        dry_run: a.dry_run,
        no_rollback: a.no_rollback,
        max_steps: a.max_steps,
        strength,
        args: a.args.clone(),
    };
    cigarscript::run::run_file(&ctx, &a.file, &rf)
}

fn check_cmd(top: &Cli, a: &CheckArgs) -> i32 {
    let mut flags = Flags {
        paths: a.paths.clone(),
        strength: None,
        json: top.json,
        plain: top.plain,
        no_color: top.no_color,
    };
    let ctx = ctx_from(&flags);
    if let Some(s) = a.strength.as_deref() {
        match Strength::parse(s) {
            Some(st) => flags.strength = Some(st),
            None => {
                // A usage error like any other: a code, a hint, and one
                // JSON object under --json.
                let d = cigscript::diagnostics::usage(format!(
                    "--strength is `{s}`, want `mild`, `medium` or `full`"
                ))
                .code("E800")
                .with_hint("cigar check --strength mild, medium or full");
                cigscript::cli::report(&ctx, &d, None, None);
                return exit::USAGE;
            }
        }
    }
    run_check(&ctx, &flags)
}
