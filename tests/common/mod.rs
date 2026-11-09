//! Shared helpers for the integration tests.

#![allow(dead_code)]

use cigarscript::check::{check_source, Checked, Kind as FileKind};
use cigarscript::features::Strength;
use cigscript::diagnostics::Diagnostic;

/// Check a file as a `.cig`.
pub fn as_cig(source: &str) -> Checked {
    check_source(FileKind::Cig, source, None)
}

/// Check a file as a `.cigar`.
pub fn as_cigar(source: &str, strength: Option<Strength>) -> Checked {
    check_source(FileKind::Cigar, source, strength)
}

/// What cigscript's own `parse + check` makes of a source. The exact
/// behaviour `cig check` has.
pub fn cigscript_result(source: &str) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    match cigscript::syntax::parse(source) {
        Ok(program) => {
            let rep = cigscript::check::check(&program, &[]);
            (rep.errors, rep.warnings)
        }
        Err(d) => (vec![d], Vec::new()),
    }
}

/// Compare two diagnostic lists by what the user sees: code, kind,
/// (line, col). Messages change freely across versions.
pub fn fingerprints(diags: &[Diagnostic]) -> Vec<(String, String, Option<u32>, Option<u32>)> {
    diags
        .iter()
        .map(|d| {
            (
                d.code.to_string(),
                d.kind.as_str().to_string(),
                d.line,
                d.col,
            )
        })
        .collect()
}

/// Walk two directories (optional) and return every `.cig` file at the
/// top level, sorted. The test skips with a printed note when the
/// directory does not exist.
pub fn cigscript_corpus(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for sub in ["examples", "tests/scripts"] {
        let d = root.join(sub);
        if !d.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("cig") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// The CigScript source checkout, from `CIGSCRIPT_SRC`.
pub fn cigscript_src() -> Option<std::path::PathBuf> {
    let p = std::env::var_os("CIGSCRIPT_SRC")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/home/jay/CigScript"));
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}
