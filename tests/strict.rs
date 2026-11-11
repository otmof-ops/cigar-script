// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Held to Rust's standard (#22, S1-S4): programs the checker must refuse,
//! each with the error it names first, and the programs beside them that
//! must still pass. `tests/strict/sN-NAME.cigar` + `.err` (code on line 1,
//! a substring of the message on line 2) is refused; `ok-sN-NAME.cigar`
//! checks clean.

use std::path::{Path, PathBuf};

use cigarscript::check::{check_file_source, Kind};

fn cases(prefix: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/strict");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cigar"))
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with(prefix) || n.starts_with(&format!("ok-{prefix}"))
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no cases for {prefix}");
    out
}

/// Every case with the prefix: a refused one fails first with its error,
/// an `ok-` one checks clean.
fn run(prefix: &str) {
    let mut wrong = Vec::new();
    for case in cases(prefix) {
        let name = case.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&case).unwrap();
        let checked = check_file_source(Kind::Cigar, &src, None, Some(&case));
        if name.starts_with("ok-") {
            if let Some(d) = checked.errors.first() {
                wrong.push(format!(
                    "{name}: should check, and gives {} {}",
                    d.code, d.message
                ));
            }
            continue;
        }
        let err = std::fs::read_to_string(case.with_extension("err")).unwrap();
        let mut lines = err.lines();
        let (code, part) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
        match checked.errors.first() {
            Some(d)
                if d.code == code
                    && (d.message.contains(part)
                        || d.hint.as_deref().unwrap_or("").contains(part)) => {}
            Some(d) => wrong.push(format!(
                "{name}: want {code} containing `{part}`, got {} {} / {:?}",
                d.code, d.message, d.hint
            )),
            None => wrong.push(format!("{name}: want {code}, and it checks")),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn optional_must_be_narrowed() {
    run("s3-");
}

#[test]
fn every_path_returns() {
    run("s4-");
}

#[test]
fn mutable_containers_are_invariant() {
    run("s1-");
}

#[test]
fn function_parameters_are_contravariant() {
    run("s2-");
}
