// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar run`: check a file, then run what it lowers to as a cig program.
//!
//! A cig program (cig's docs/PROGRAMS.md) is cig's runtime with a script
//! appended, the way cigcompile builds one: it runs exactly as `cig run`
//! would run the script, with plans, journals, rollback and run records,
//! and a run-time error names the file and line it came from. `cigar`
//! carries cig's runtime, so the program is `cigar` itself with the lowered
//! script appended; it is kept in cigar's cache, named by its content, so
//! running the same file again starts at once.

use std::fs;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cigscript::cli::Ctx;
use cigscript::diagnostics::usage;
use cigscript::program::Program;

use crate::check::{check_file_source, Kind as FileKind};
use crate::cli::exit;
use crate::features::Strength;

/// What `cigar run` was asked for, besides the file.
#[derive(Default)]
pub struct RunFlags {
    pub dry_run: bool,
    pub no_rollback: bool,
    pub max_steps: Option<u64>,
    pub strength: Option<Strength>,
    /// The script's own arguments (`args` in the script).
    pub args: Vec<String>,
}

/// How many built programs the cache keeps.
const KEEP: usize = 12;

/// Check `path`; when it checks, run it.
pub fn run_file(ctx: &Ctx, path: &Path, f: &RunFlags) -> i32 {
    let Some(kind) = FileKind::from_path(path) else {
        let d = usage(format!("{}: not a .cig or .cigar file", path.display()))
            .code("E800")
            .with_hint("name a .cig script or a .cigar file");
        cigscript::cli::report(ctx, &d, None, None);
        return exit::USAGE;
    };
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            let d = usage(format!("cannot read {}: {e}", path.display()))
                .code("E801")
                .with_hint("name a file you can read");
            cigscript::cli::report(ctx, &d, None, None);
            return exit::USAGE;
        }
    };
    let strength = if kind == FileKind::Cig {
        None
    } else {
        f.strength
    };
    let checked = check_file_source(kind, &source, strength, Some(path));
    let name = path.to_string_lossy().to_string();
    if !checked.ok() {
        if ctx.json {
            for d in &checked.errors {
                cigscript::cli::report(ctx, d, Some(&name), Some(&checked.source_as_written));
            }
        } else {
            cigscript::cli::report_all(ctx, &checked.errors, &name, &checked.source_as_written);
        }
        return exit::CHECK;
    }
    if !ctx.json {
        for w in &checked.warnings {
            eprint!(
                "{}",
                ctx.yellow(
                    &w.render(Some(&name), Some(&checked.source_as_written))
                        .replacen("error[", "warning[", 1)
                )
            );
        }
    }
    if !ctx.json {
        // Under --json the program's own output owns stdout.
        crate::cli::report_hints(ctx, &name, &checked);
    }
    if kind == FileKind::Cigar && checked.strength == Strength::Full {
        let root = crate::experience::project_root(path);
        let said = crate::experience::confirm_full(
            &crate::experience::Dirs::from_env(),
            &root,
            crate::experience::interactive(),
            &mut io::stdin().lock(),
            &mut io::stderr(),
        );
        if said == crate::experience::Outcome::Declined {
            return exit::USAGE;
        }
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| cigscript::program::good_name(s))
        .unwrap_or_else(|| "script".to_string());
    let program = Program::new(
        &stem,
        &name,
        &checked.source_as_cigscript_saw_it,
        &format!("cigar {}", env!("CARGO_PKG_VERSION")),
    );
    let exe = match built(&program) {
        Ok(p) => p,
        Err(e) => {
            let d = usage(format!("cannot build the program to run {name}: {e}"))
                .code("E802")
                .with_hint(
                    "cigar keeps built programs in its cache folder; check it can be written",
                );
            cigscript::cli::report(ctx, &d, None, None);
            return exit::USAGE;
        }
    };
    let mut cmd = Command::new(&exe);
    if f.dry_run {
        cmd.arg("--dry-run");
    }
    if f.no_rollback {
        cmd.arg("--no-rollback");
    }
    if let Some(n) = f.max_steps {
        cmd.arg("--max-steps").arg(n.to_string());
    }
    if ctx.json {
        cmd.arg("--json");
    }
    if ctx.plain {
        cmd.arg("--plain");
    }
    if !ctx.color && !io::stderr().is_terminal() {
        cmd.arg("--no-color");
    }
    cmd.arg("--").args(&f.args);
    let _ = io::stdout().flush();
    // Its errors come through cigar, which shows each place as it is
    // written in the file rather than as it was lowered.
    if checked.map.is_some() && !ctx.json {
        cmd.stderr(Stdio::piped());
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let d = usage(format!("cannot start the program built from {name}: {e}")).code("E802");
            cigscript::cli::report(ctx, &d, None, None);
            return exit::USAGE;
        }
    };
    let err = child.stderr.take();
    let waited = crate::phases::phase("run", || {
        if let (Some(err), Some(map)) = (err, checked.map.as_ref()) {
            relay(
                err,
                &name,
                map,
                &checked.source_as_cigscript_saw_it,
                &checked.source_as_written,
            );
        }
        child.wait()
    });
    match waited {
        Ok(s) => s.code().unwrap_or(1),
        Err(_) => 1,
    }
}

/// The offset of each line's start.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// The text of line `n` (1-based), without its line end.
fn line_of(text: &str, starts: &[usize], n: usize) -> Option<String> {
    let s = *starts.get(n.checked_sub(1)?)?;
    let e = starts.get(n).map(|e| e - 1).unwrap_or(text.len());
    Some(text.get(s..e.max(s))?.trim_end_matches('\r').to_string())
}

/// Line and column (1-based, in characters) of an offset.
fn line_col(text: &str, starts: &[usize], at: usize) -> (usize, usize) {
    let line = starts.partition_point(|&s| s <= at);
    let col = text
        .get(starts[line - 1]..at)
        .map(|t| t.chars().count())
        .unwrap_or(0)
        + 1;
    (line, col)
}

/// Copy the program's error output, rewriting each place cig shows in the
/// lowered text (`--> file:L:C`, the line, the caret) to the file as
/// written. A place in what cigar appended after the file is left as it is.
fn relay(err: impl Read, name: &str, map: &crate::check::SourceMap, lowered: &str, written: &str) {
    let (ls, ws) = (line_starts(lowered), line_starts(written));
    let arrow = format!("  --> {name}:");
    let mut out = io::stderr();
    // An error being rewritten: its lowered offset, and where it is written.
    let mut open: Option<(usize, usize, usize, usize, bool)> = None;
    let mut reader = BufReader::new(err);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match reader.read_until(b'\n', &mut raw) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&raw);
        let line = line.trim_end_matches(['\n', '\r']).to_string();
        if let Some(rest) = line.strip_prefix(&arrow) {
            let mut nums = rest.splitn(2, ':').map(|n| n.trim().parse::<usize>().ok());
            if let (Some(Some(l)), Some(Some(c))) = (nums.next(), nums.next()) {
                let at = ls.get(l.wrapping_sub(1)).and_then(|&s| {
                    let t = lowered.get(s..)?;
                    Some(
                        s + t
                            .char_indices()
                            .nth(c.saturating_sub(1))
                            .map(|(i, _)| i)
                            .unwrap_or(0),
                    )
                });
                if let Some((p, o)) = at.and_then(|p| map.original(p).map(|o| (p, o))) {
                    let own = map.written(p);
                    let (wl, mut wc) = line_col(written, &ws, o);
                    if !own {
                        // Text the lowering wrote: the whole statement.
                        let text = line_of(written, &ws, wl).unwrap_or_default();
                        wc = text.chars().take_while(|c| c.is_whitespace()).count() + 1;
                    }
                    let _ = writeln!(out, "{arrow}{wl}:{wc}");
                    open = Some((p, o, wl, wc, own));
                    continue;
                }
            }
            open = None;
        } else if let Some((p, o, wl, wc, own)) = open {
            let gutter = line
                .find(" |")
                .filter(|&i| line[..i].trim().chars().all(|c| c.is_ascii_digit()));
            match gutter {
                Some(i) if !line[..i].trim().is_empty() => {
                    // `L | text`: the line as written.
                    let text = line_of(written, &ws, wl).unwrap_or_default();
                    let _ = writeln!(out, "{:>w$} | {text}", wl, w = i);
                    continue;
                }
                Some(i) if line[i + 2..].trim_start().starts_with('^') => {
                    // The caret, under the same thing in the line as written.
                    let text = line_of(written, &ws, wl).unwrap_or_default();
                    let carets = line[i + 2..].trim().chars().filter(|c| *c == '^').count();
                    let len = if own {
                        let end = map
                            .original(p + carets)
                            .filter(|e| *e > o)
                            .unwrap_or(o + carets);
                        written
                            .get(o..end)
                            .map(|t| t.lines().next().unwrap_or("").chars().count())
                            .unwrap_or(1)
                    } else {
                        text.trim().chars().count()
                    };
                    let _ = writeln!(
                        out,
                        "{} | {}{}",
                        " ".repeat(i),
                        " ".repeat(wc - 1),
                        "^".repeat(len.max(1))
                    );
                    open = None;
                    continue;
                }
                Some(_) => {}
                None => open = None,
            }
        }
        let _ = writeln!(out, "{line}");
    }
}

/// Where built programs are kept.
pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            if cfg!(windows) {
                std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
            } else {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"))
            }
        })
        .unwrap_or_else(std::env::temp_dir);
    base.join("cigar").join("run")
}

/// The program's executable: cigar with the script appended, built once
/// per script and cigar.
fn built(program: &Program) -> io::Result<PathBuf> {
    let me = std::env::current_exe()?;
    let meta = fs::metadata(&me)?;
    let stamp = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // The script's hash, and which cigar built it.
    // The script (its whole hash), the file it names in errors and records,
    // what built it, and which cigar it is appended to.
    let key = format!(
        "{}-{:016x}-{:x}-{:x}",
        program.sha256,
        fnv(format!("{}\0{}", program.file, program.compiler).as_bytes()),
        meta.len(),
        stamp
    );
    let dir = cache_dir();
    private_dir(&dir)?;
    let path = dir.join(format!(
        "{}-{key}{}",
        program.name,
        std::env::consts::EXE_SUFFIX
    ));
    if fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
        // Used now: the newest, so the cache keeps it.
        if let Ok(f) = fs::File::options().append(true).open(&path) {
            let _ = f.set_modified(std::time::SystemTime::now());
        }
        return Ok(path);
    }
    let runtime = fs::read(&me)?;
    let bytes = cigscript::program::append(&runtime, program);
    let tmp = dir.join(format!(".{}-{key}.{}", program.name, std::process::id()));
    // A new file, never one already there (nor what a link there points at).
    let mut open = fs::OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.mode(0o700);
    }
    let mut f = open.open(&tmp)?;
    f.write_all(&bytes)?;
    drop(f);
    fs::rename(&tmp, &path)?;
    prune(&dir, &path);
    Ok(path)
}

/// The folder built programs are kept in, made if need be: a real folder
/// only its owner can write to, so nothing else can put a program there for
/// cigar to start.
fn private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let m = fs::symlink_metadata(dir)?;
    if !m.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a folder",
            dir.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Only its owner can change its mode: this fails on a folder that
        // is someone else's.
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// FNV-1a, 64 bits: tells apart two files whose scripts are the same.
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Keep the newest built programs, and the one about to run.
fn prune(dir: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        // Not a program another cigar is still writing (`.name-key.pid`).
        .filter(|p| p.is_file() && p != keep)
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .filter_map(|p| {
            fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| (t, p))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(KEEP.saturating_sub(1)) {
        let _ = fs::remove_file(p);
    }
}

/// Does this executable carry a program (it is one `cigar run` built)?
/// Only the end of the file is read.
pub fn carries_program() -> bool {
    let Ok(me) = std::env::current_exe() else {
        return false;
    };
    let Ok(mut f) = fs::File::open(me) else {
        return false;
    };
    !matches!(cigscript::program::read(&mut f), Ok(None))
}
