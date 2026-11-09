// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! `cigar`: the CigarScript command line. For now, one command: `cigar
//! check`, which reports on a file (or a folder's `.cig` and `.cigar`
//! files at the top level) the way `cig check` does.

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
    }
}

fn check_cmd(top: &Cli, a: &CheckArgs) -> i32 {
    let strength = match a.strength.as_deref() {
        Some(s) => match Strength::parse(s) {
            Some(s) => Some(s),
            None => {
                eprintln!("cigar check: --strength is `{s}`, want `mild`, `medium` or `full`");
                return exit::USAGE;
            }
        },
        None => None,
    };
    let flags = Flags {
        paths: a.paths.clone(),
        strength,
        json: top.json,
        plain: top.plain,
        no_color: top.no_color,
    };
    let ctx = ctx_from(&flags);
    run_check(&ctx, &flags)
}
