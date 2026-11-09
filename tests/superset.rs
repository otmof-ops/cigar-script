//! Every CigScript script is mild CigarScript (issue #1, the superset).
//!
//! These tests look for the CigScript source checkout at
//! `CIGSCRIPT_SRC` (default `/home/jay/CigScript`) and skip, printing a
//! note, when it is missing.

mod common;

use cigarscript::features::Strength;
use common::{as_cig, as_cigar, cigscript_corpus, cigscript_result, cigscript_src, fingerprints};

/// Files that use a reserved CigarScript word as an ordinary name. Each
/// is expected to get an E1101 or E1102 naming that word when checked
/// as `.cigar`. In v1.18.0 of CigScript, the reserved words appear only
/// inside comments in the corpus, so the list is empty. When a future
/// cig release uses one of these as a real identifier, add the file
/// name and the word here.
const RESERVED_WORD_FILES: &[(&str, &str)] = &[];

fn print_skip(reason: &str) {
    eprintln!("note: skipping; {reason}");
}

#[test]
fn every_cigscript_script_is_mild_cigarscript() {
    let Some(root) = cigscript_src() else {
        print_skip("CIGSCRIPT_SRC does not point at a CigScript checkout");
        return;
    };
    let scripts = cigscript_corpus(&root);
    if scripts.is_empty() {
        print_skip("the checkout has no .cig files at the top of examples/ or tests/scripts/");
        return;
    }
    let mut diffs: Vec<String> = Vec::new();
    for path in &scripts {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                diffs.push(format!("{}: cannot read: {e}", path.display()));
                continue;
            }
        };
        let (cig_errors, cig_warnings) = cigscript_result(&source);
        let checked = as_cig(&source);
        let left = fingerprints(&cig_errors);
        let right = fingerprints(&checked.errors);
        if left != right {
            diffs.push(format!(
                "{}: errors differ\n  cig = {left:?}\n  cigar = {right:?}",
                path.display()
            ));
        }
        let lw = fingerprints(&cig_warnings);
        let rw = fingerprints(&checked.warnings);
        if lw != rw {
            diffs.push(format!(
                "{}: warnings differ\n  cig = {lw:?}\n  cigar = {rw:?}",
                path.display()
            ));
        }
    }
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn as_cigar_mild_matches_cig() {
    let Some(root) = cigscript_src() else {
        print_skip("CIGSCRIPT_SRC does not point at a CigScript checkout");
        return;
    };
    let scripts = cigscript_corpus(&root);
    if scripts.is_empty() {
        print_skip("the checkout has no .cig files");
        return;
    }
    let reserved: std::collections::HashMap<&str, &str> =
        RESERVED_WORD_FILES.iter().copied().collect();
    let mut diffs: Vec<String> = Vec::new();
    for path in &scripts {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                diffs.push(format!("{}: cannot read: {e}", path.display()));
                continue;
            }
        };
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let checked = as_cigar(&source, None);
        if let Some(word) = reserved.get(name.as_str()) {
            // Expected: at least one E1101 or E1102 whose message mentions the word.
            let hits: Vec<_> = checked
                .errors
                .iter()
                .filter(|d| (d.code == "E1101" || d.code == "E1102") && d.message.contains(word))
                .collect();
            if hits.is_empty() {
                diffs.push(format!(
                    "{}: expected E1101/E1102 naming `{word}`, got {:?}",
                    path.display(),
                    fingerprints(&checked.errors)
                ));
            }
            continue;
        }
        let (cig_errors, _) = cigscript_result(&source);
        let left = fingerprints(&cig_errors);
        let right = fingerprints(&checked.errors);
        if left != right {
            diffs.push(format!(
                "{}: errors differ from cig's\n  cig = {left:?}\n  cigar = {right:?}",
                path.display()
            ));
        }
    }
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn turning_up_is_additive() {
    let Some(root) = cigscript_src() else {
        print_skip("CIGSCRIPT_SRC does not point at a CigScript checkout");
        return;
    };
    let scripts = cigscript_corpus(&root);
    if scripts.is_empty() {
        print_skip("the checkout has no .cig files");
        return;
    }
    let reserved: std::collections::HashSet<&str> =
        RESERVED_WORD_FILES.iter().map(|(n, _)| *n).collect();
    let mut diffs: Vec<String> = Vec::new();
    for path in &scripts {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                diffs.push(format!("{}: cannot read: {e}", path.display()));
                continue;
            }
        };
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if reserved.contains(name.as_str()) {
            continue;
        }
        let mild = as_cigar(&source, None);
        for turned_up in [Strength::Medium, Strength::Full] {
            let with_decl = format!("strength {turned_up}\n{source}");
            let turned = as_cigar(&with_decl, None);
            // Account for the one-line shift in position.
            let expected: Vec<_> = fingerprints(&mild.errors)
                .into_iter()
                .map(|(code, kind, line, col)| (code, kind, line.map(|l| l + 1), col))
                .collect();
            let got = fingerprints(&turned.errors);
            if expected != got {
                diffs.push(format!(
                    "{}: turning up to {turned_up} changed the errors\n  mild+1 = {expected:?}\n  {turned_up} = {got:?}",
                    path.display()
                ));
            }
        }
    }
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}
