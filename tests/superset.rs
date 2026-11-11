// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Every CigScript script is mild CigarScript (issue #1, the superset).
//!
//! These tests look for the CigScript source checkout at
//! `CIGSCRIPT_SRC` (default `../cig-script`) and skip, printing a
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

// ---- a latent error in a `.cig` file is a hint, and nothing else (#1) ----

/// A `.cig` file whose call has the wrong number of arguments. CigScript
/// accepts it and fails only when the line runs, after "before" is printed;
/// at strength medium it is an error (E1216).
const LATENT: &str = "pull add(a, b) { snuff a + b }\nexhale \"before\"\nstick r = add(1)\n";

const HINT: &str = "this would be an error at strength medium: `add` takes 2 argument(s), given 1";

struct Ran {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run `bin args` in a scratch home with a hard timeout.
fn run_in(home: &std::path::Path, bin: &std::path::Path, args: &[&std::ffi::OsStr]) -> Ran {
    #[cfg(unix)]
    let mut cmd = {
        let mut c = std::process::Command::new("timeout");
        c.arg("60").arg(bin);
        c
    };
    #[cfg(not(unix))]
    let mut cmd = std::process::Command::new(bin);
    let out = cmd
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CIGSCRIPT_HOME", home.join("cig-home"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("LOCALAPPDATA", home.join("local"))
        .env("APPDATA", home.join("roaming"))
        .env("NO_COLOR", "1")
        .env("CIG_DOCTOR", "0")
        .output()
        .expect("runs");
    Ran {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn cigar() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_cigar"))
}

#[test]
fn a_latent_error_in_a_cig_file_is_a_hint_in_the_check() {
    let checked = as_cig(LATENT);
    // CigScript's acceptance is untouched: no new error, no new warning.
    let (cig_errors, cig_warnings) = cigscript_result(LATENT);
    assert!(cig_errors.is_empty() && cig_warnings.is_empty());
    assert!(checked.ok(), "{:?}", fingerprints(&checked.errors));
    assert!(checked.warnings.is_empty());
    assert_eq!(checked.hints.len(), 1, "{:?}", checked.hints);
    let h = &checked.hints[0];
    assert_eq!(h.code, "E1216");
    assert_eq!(h.message, HINT);
    assert_eq!((h.line, h.col), (Some(3), Some(11)));
    // The same text as a `.cigar` file at medium says it as an error, and
    // at mild says nothing: only a `.cig` file gets hints.
    let medium = as_cigar(&format!("strength medium\n{LATENT}"), None);
    assert_eq!(fingerprints(&medium.errors).len(), 1);
    assert!(as_cigar(LATENT, None).hints.is_empty());
    assert!(medium.hints.is_empty());
}

#[test]
fn a_clean_cig_file_gets_no_hint() {
    let checked = as_cig("pull add(a, b) { snuff a + b }\nexhale add(1, 2)\n");
    assert!(checked.ok() && checked.hints.is_empty());
}

#[test]
fn a_cig_file_cig_rejects_gets_its_own_error_and_no_hint() {
    let checked = as_cig("exhale (\n");
    assert!(!checked.ok());
    assert!(checked.hints.is_empty());
}

#[test]
fn a_latent_error_runs_as_cig_runs_it_and_prints_the_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let cig = home.join("latent.cig");
    std::fs::write(&cig, LATENT).unwrap();
    // The same program as a mild `.cigar` file: what `cig run` does with
    // it, with no hint, is the reference.
    let mild = home.join("latent.cigar");
    std::fs::write(&mild, LATENT).unwrap();

    let reference = run_in(home, &cigar(), &["run".as_ref(), mild.as_os_str()]);
    let got = run_in(home, &cigar(), &["run".as_ref(), cig.as_os_str()]);
    assert_eq!(got.stdout, "before\n", "stderr: {}", got.stderr);
    assert_eq!(got.stdout, reference.stdout);
    assert_ne!(got.code, Some(0), "the call still fails when it runs");
    assert_eq!(got.code, reference.code);
    assert!(got.stderr.contains(HINT), "{}", got.stderr);
    assert!(got.stderr.contains("hint[E1216"), "{}", got.stderr);
    assert!(!reference.stderr.contains("would be an error"));

    // And as the installed `cig` runs it, when the test is told where it is.
    if let Some(cig_bin) = std::env::var_os("CIG_BIN").map(std::path::PathBuf::from) {
        let cigs = run_in(home, &cig_bin, &["run".as_ref(), cig.as_os_str()]);
        assert_eq!(got.stdout, cigs.stdout);
        assert_eq!(got.code, cigs.code);
    }
}

#[test]
fn check_prints_the_hint_and_still_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let f = home.join("latent.cig");
    std::fs::write(&f, LATENT).unwrap();
    let got = run_in(home, &cigar(), &["check".as_ref(), f.as_os_str()]);
    assert_eq!(got.code, Some(0), "{}", got.stderr);
    assert!(got.stderr.contains(HINT), "{}", got.stderr);
    assert!(got.stderr.contains("hint[E1216 check]"), "{}", got.stderr);
    assert!(
        !got.stderr.contains("error["),
        "a hint is never an error: {}",
        got.stderr
    );
    assert!(got.stdout.contains("checked"), "{}", got.stdout);
}
