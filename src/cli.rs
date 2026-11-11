// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The `cigar check` command line.
//!
//! One file, or a folder's `.cig` and `.cigar` files at the top level.
//! Diagnostics are rendered through cigscript's own `cli::report`, so
//! the output looks like `cig`'s. `--json` writes each diagnostic as
//! one JSON object per line, the same shape `cig check --json` uses.

use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use cigscript::cli::Ctx;
use cigscript::diagnostics::{usage, Diagnostic};

use crate::check::{check_file_source_deny, Checked, Deny, Kind as FileKind};
use crate::features::Strength;

/// Exit codes, matching `cigcompile build`'s check path (otmof-ops/cig-compile).
pub mod exit {
    pub const OK: i32 = 0;
    pub const CHECK: i32 = 2;
    pub const USAGE: i32 = 3;
}

/// Flags the top-level entry parses out of argv.
pub struct Flags {
    pub paths: Vec<PathBuf>,
    pub strength: Option<Strength>,
    pub json: bool,
    pub plain: bool,
    pub no_color: bool,
    /// Which warnings are errors: `--deny any`, `--deny warnings`.
    pub deny: Deny,
}

/// Build a cigscript `Ctx` from flags, the way cig-compile does.
pub fn ctx_from(flags: &Flags) -> Ctx {
    Ctx {
        json: flags.json,
        color: !flags.no_color
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stderr().is_terminal(),
        doctor: false,
        plain: flags.plain || cigscript::cli::plain_by_env(),
    }
}

/// Run `cigar check` over every path, in argv order. Each file's
/// diagnostics are reported through cigscript's hook; a file with any
/// error makes the overall exit `CHECK`.
pub fn run_check(ctx: &Ctx, flags: &Flags) -> i32 {
    if flags.paths.is_empty() {
        let d = usage("cigar check: name a file or a folder").code("E800");
        cigscript::cli::report(ctx, &d, None, None);
        return exit::USAGE;
    }
    let mut had_error = false;
    let mut had_usage = false;
    for path in &flags.paths {
        match files_of(path) {
            Ok(files) => {
                if files.is_empty() {
                    let d = usage(format!(
                        "{}: no .cig or .cigar files at the top of the folder",
                        path.display()
                    ))
                    .code("E800");
                    cigscript::cli::report(ctx, &d, None, None);
                    had_usage = true;
                    continue;
                }
                for file in files {
                    match check_file(ctx, &file, flags.strength, flags.deny) {
                        FileOutcome::Ok => {}
                        FileOutcome::Check => had_error = true,
                        FileOutcome::Usage => had_usage = true,
                    }
                }
            }
            Err(d) => {
                cigscript::cli::report(ctx, &d, None, None);
                had_usage = true;
            }
        }
    }
    if had_usage {
        return exit::USAGE;
    }
    if had_error {
        return exit::CHECK;
    }
    exit::OK
}

enum FileOutcome {
    Ok,
    Check,
    Usage,
}

fn check_file(ctx: &Ctx, path: &Path, strength: Option<Strength>, deny: Deny) -> FileOutcome {
    let kind = match FileKind::from_path(path) {
        Some(k) => k,
        None => {
            let d = usage(format!("{}: not a .cig or .cigar file", path.display()))
                .code("E800")
                .with_hint("name a .cig script or a .cigar file");
            cigscript::cli::report(ctx, &d, None, None);
            return FileOutcome::Usage;
        }
    };
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            let d = usage(format!("cannot read {}: {e}", path.display()))
                .code("E801")
                .with_hint("name a file you can read");
            cigscript::cli::report(ctx, &d, None, None);
            return FileOutcome::Usage;
        }
    };
    let effective = if matches!(kind, FileKind::Cig) {
        None
    } else {
        strength
    };
    let checked: Checked = check_file_source_deny(kind, &source, effective, Some(path), deny);
    let name = path.to_string_lossy().to_string();
    report_checked(ctx, &name, &checked);
    if checked.ok() {
        FileOutcome::Ok
    } else {
        FileOutcome::Check
    }
}

/// Say each hint a `.cig` file earned from the deeper checker: on stderr,
/// through cig's renderer, as a hint and never as an error. Under `--json`
/// each is one diagnostic object, as the warnings are.
pub fn report_hints(ctx: &Ctx, name: &str, checked: &Checked) {
    for h in &checked.hints {
        if ctx.json {
            cigscript::cli::report(ctx, h, Some(name), Some(&checked.source_as_written));
        } else {
            eprint!(
                "{}",
                ctx.yellow(
                    &h.render(Some(name), Some(&checked.source_as_written))
                        .replacen("error[", "hint[", 1),
                ),
            );
        }
    }
}

pub fn report_checked(ctx: &Ctx, name: &str, checked: &Checked) {
    if ctx.json {
        // One diagnostic per line, each carrying the file. Matches the
        // shape cig's reports emit.
        for d in &checked.errors {
            cigscript::cli::report(ctx, d, Some(name), Some(&checked.source_as_written));
        }
        for w in &checked.warnings {
            cigscript::cli::report(ctx, w, Some(name), Some(&checked.source_as_written));
        }
        report_hints(ctx, name, checked);
        if checked.ok() {
            // `cig check --json` prints one summary object per file;
            // match that shape at the top level.
            let obj = serde_json::json!({
                "file": name,
                "ok": true,
            });
            println!("{obj}");
        }
        return;
    }
    if checked.errors.is_empty() {
        for w in &checked.warnings {
            eprint!(
                "{}",
                ctx.yellow(
                    &w.render(Some(name), Some(&checked.source_as_written))
                        .replacen("error[", "warning[", 1),
                ),
            );
        }
        report_hints(ctx, name, checked);
        println!("checked {name}");
        return;
    }
    cigscript::cli::report_all(ctx, &checked.errors, name, &checked.source_as_written);
}

/// The top-level files a path names: one file, or the `.cig` and
/// `.cigar` files at the top of a folder.
#[allow(clippy::result_large_err)]
fn files_of(path: &Path) -> Result<Vec<PathBuf>, Diagnostic> {
    let meta = fs::metadata(path).map_err(|e| {
        usage(format!("{}: {e}", path.display()))
            .code("E801")
            .with_hint("name a file or a folder")
    })?;
    if meta.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !meta.is_dir() {
        return Err(usage(format!("{}: neither a file nor a folder", path.display())).code("E800"));
    }
    let mut out = Vec::new();
    let entries =
        fs::read_dir(path).map_err(|e| usage(format!("{}: {e}", path.display())).code("E801"))?;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if FileKind::from_path(&p).is_some() {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}
