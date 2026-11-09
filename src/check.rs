//! Checking one file: the pipeline cigscript's `check` sits inside.
//!
//! A `.cig` file goes straight to cigscript's parse + check. A `.cigar`
//! file has its `strength` declaration read, the declaration blanked,
//! and the blanked source scanned for features above the effective
//! strength. If the scan finds anything, we stop there; a clean scan
//! then hands the blanked source to cigscript's parse + check, exactly
//! as `cig check` would.

use std::path::Path;

use cigscript::check::Report;
use cigscript::diagnostics::Diagnostic;

use crate::features::Strength;
use crate::gate;
use crate::strength;

/// Which language the file is: `.cig` is always mild CigScript; `.cigar`
/// has a declared strength.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cig,
    Cigar,
}

impl Kind {
    pub fn from_path(path: &Path) -> Option<Kind> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("cig") => Some(Kind::Cig),
            Some("cigar") => Some(Kind::Cigar),
            _ => None,
        }
    }
}

/// The outcome of a check: the effective strength, the source cigscript
/// saw (the blanked source for a `.cigar` file, the original for a
/// `.cig` file), and all the diagnostics.
pub struct Checked {
    pub kind: Kind,
    pub strength: Strength,
    /// What cigscript saw. Diagnostics with a span are measured against
    /// this: it has the same byte positions and lines as the input.
    pub source_as_cigscript_saw_it: String,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

impl Checked {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Check one file.
///
/// `source` is the file's whole content. `kind` is which language the
/// file is. `override_strength`, when set, forces the effective strength
/// (for `--strength X`); a `.cig` file ignores it, because `.cig` is
/// always mild.
pub fn check_source(kind: Kind, source: &str, override_strength: Option<Strength>) -> Checked {
    match kind {
        Kind::Cig => check_cig(source),
        Kind::Cigar => check_cigar(source, override_strength),
    }
}

fn check_cig(source: &str) -> Checked {
    let (errors, warnings) = parse_and_check(source);
    Checked {
        kind: Kind::Cig,
        strength: Strength::Mild,
        source_as_cigscript_saw_it: source.to_string(),
        errors,
        warnings,
    }
}

fn check_cigar(source: &str, override_strength: Option<Strength>) -> Checked {
    // Reading the declaration can itself fail at lex time; a lex error
    // is reported as the only finding.
    let decl = match strength::read(source) {
        Ok(d) => d,
        Err(d) => {
            return Checked {
                kind: Kind::Cigar,
                strength: Strength::Mild,
                source_as_cigscript_saw_it: source.to_string(),
                errors: vec![d],
                warnings: Vec::new(),
            };
        }
    };
    let effective = override_strength.unwrap_or(decl.strength);
    // Any E1103 the reader raised comes first (they are at the top of
    // the file by definition).
    let mut errors = decl.errors;

    // Scan the blanked source for feature uses. The scan's lexer errors
    // (unlikely, given the file already tokenised once above) are
    // returned as the only finding.
    match gate::scan(&decl.blanked, effective) {
        Ok(gate_errors) => errors.extend(gate_errors),
        Err(d) => errors.push(d),
    }

    if !errors.is_empty() {
        return Checked {
            kind: Kind::Cigar,
            strength: effective,
            source_as_cigscript_saw_it: decl.blanked,
            errors,
            warnings: Vec::new(),
        };
    }

    // Clean: hand the blanked source to cigscript.
    let (errors, warnings) = parse_and_check(&decl.blanked);
    Checked {
        kind: Kind::Cigar,
        strength: effective,
        source_as_cigscript_saw_it: decl.blanked,
        errors,
        warnings,
    }
}

fn parse_and_check(source: &str) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    match cigscript::syntax::parse(source) {
        Ok(program) => {
            let rep: Report = cigscript::check::check(&program, &[]);
            (rep.errors, rep.warnings)
        }
        Err(d) => (vec![d], Vec::new()),
    }
}
