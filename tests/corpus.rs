// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! The medium and full corpus (tests/corpus/README.md): programs that run
//! and print exactly what they should, files that fail `cigar check` with
//! the error they name first, and files that check and then fail when run.
//!
//! Runs go through the real `cigar run`, with cig's state and cigar's cache
//! in a folder of their own, so a test run leaves nothing behind.

use std::path::{Path, PathBuf};
use std::process::Command;

use cigarscript::check::{check_file_source, Kind};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// Every case in a folder: `NAME.cigar`, and `NAME/main.cigar` for a case
/// with modules.
fn cases(folder: &str) -> Vec<PathBuf> {
    let dir = corpus().join(folder);
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "cigar") {
            out.push(p);
        } else if p.is_dir() && p.join("main.cigar").is_file() {
            out.push(p.join("main.cigar"));
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no cases in {}", dir.display());
    out
}

/// The `.out` or `.err` beside a case.
fn beside(case: &Path, ext: &str) -> String {
    std::fs::read_to_string(case.with_extension(ext))
        .unwrap_or_else(|e| panic!("{}: {e}", case.with_extension(ext).display()))
}

fn shown(case: &Path) -> String {
    case.strip_prefix(corpus())
        .unwrap_or(case)
        .display()
        .to_string()
}

/// `cigar run` on a case, from its folder: (exit code, stdout, stderr).
fn run(case: &Path, state: &Path) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_cigar"))
        .arg("--plain")
        .arg("run")
        .arg(case.file_name().unwrap())
        .current_dir(case.parent().unwrap())
        .env("XDG_CACHE_HOME", state.join("cache"))
        .env("CIGSCRIPT_HOME", state.join("cig"))
        .env("NO_COLOR", "1")
        .output()
        .expect("cigar runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn check(case: &Path) -> cigarscript::check::Checked {
    let src = std::fs::read_to_string(case).unwrap();
    check_file_source(Kind::Cigar, &src, None, Some(case))
}

#[test]
fn every_program_runs_and_prints_what_it_should() {
    let state = tempfile::tempdir().unwrap();
    let mut wrong = Vec::new();
    for case in cases("run") {
        let (code, out, err) = run(&case, state.path());
        let want = beside(&case, "out");
        if code != 0 || out != want {
            wrong.push(format!(
                "{}: exit {code}\n--- want\n{want}--- got\n{out}--- stderr\n{err}",
                shown(&case)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_check_case_fails_first_with_its_error() {
    let mut wrong = Vec::new();
    for case in cases("check") {
        let err = beside(&case, "err");
        let mut lines = err.lines();
        let (code, part) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
        let checked = check(&case);
        match checked.errors.first() {
            Some(d) if d.code == code && d.message.contains(part) => {}
            Some(d) => wrong.push(format!(
                "{}: want {code} containing `{part}`, got {} {}",
                shown(&case),
                d.code,
                d.message
            )),
            None => wrong.push(format!("{}: want {code}, and it checks", shown(&case))),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_runtime_case_checks_then_fails_with_its_error() {
    let state = tempfile::tempdir().unwrap();
    let mut wrong = Vec::new();
    for case in cases("runtime") {
        let err = beside(&case, "err");
        let mut lines = err.lines();
        let (code, part) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
        let checked = check(&case);
        if let Some(d) = checked.errors.first() {
            wrong.push(format!(
                "{}: should check, and gives {} {}",
                shown(&case),
                d.code,
                d.message
            ));
            continue;
        }
        let (status, _, stderr) = run(&case, state.path());
        if status == 0 || !stderr.contains(code) || !stderr.contains(part) {
            wrong.push(format!(
                "{}: want a failed run with {code} and `{part}`, got exit {status}\n{stderr}",
                shown(&case)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A boundary check coughs on the line of the binding, not inside the
/// helpers appended after the file.
#[test]
fn a_boundary_error_points_at_its_own_line() {
    let state = tempfile::tempdir().unwrap();
    let case = corpus().join("runtime/e1210-untyped-result-into-stick.cigar");
    let (_, _, stderr) = run(&case, state.path());
    let line = std::fs::read_to_string(&case)
        .unwrap()
        .lines()
        .position(|l| l.contains("stick n: Int"))
        .unwrap()
        + 1;
    let at = format!("e1210-untyped-result-into-stick.cigar:{line}:");
    assert!(stderr.contains(&at), "want {at} in\n{stderr}");
}
