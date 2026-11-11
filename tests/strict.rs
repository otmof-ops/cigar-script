// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Held to Rust's standard (#22, S1-S7): programs the checker must refuse,
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

#[test]
fn reserved_words_are_names_after_dot_and_as_keys() {
    run("d1-");
}

fn warning_codes(src: &str) -> Vec<(String, String)> {
    let c = check_file_source(Kind::Cigar, src, None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    c.warnings
        .iter()
        .map(|w| (w.code.to_string(), w.message.clone()))
        .collect()
}

/// A name that shadows a contextual keyword is W1234 until the file's
/// strength reserves the word: at medium, `match` is an error and `layer`
/// (a full-strength word) is a warning naming the strength it turns on.
#[test]
fn a_name_that_shadows_a_keyword_warns_w1234() {
    let mild = warning_codes("stick match = 1\nexhale match\n");
    assert_eq!(mild.len(), 2, "{mild:?}");
    assert!(mild.iter().all(|(c, m)| c == "W1234"
        && m.contains("`match` will be a keyword if this file goes to medium")));

    let medium = warning_codes("strength medium\nstick layer = 1\nexhale layer\n");
    assert_eq!(medium.len(), 2, "{medium:?}");
    assert!(medium[0].1.contains("goes to full"), "{medium:?}");

    // Fields and keys are not names that shadow anything.
    assert!(warning_codes("stick m = {match: 1}\nexhale m.match\n").is_empty());
    // A keyword where its syntax begins is the feature, not a shadow.
    let c = check_file_source(Kind::Cigar, "record R { x: 1 }\n", None, None);
    assert_eq!(c.errors[0].code, "E1101");
}

/// L9: the message names the word and the escape, and the escape works.
#[test]
fn the_reserved_word_message_offers_r_hash() {
    let c = check_file_source(Kind::Cigar, "strength medium\nstick enum = 1\n", None, None);
    let d = &c.errors[0];
    assert_eq!(d.code, "E1105");
    assert_eq!(
        d.message,
        "`enum` is a reserved word in `.cigar` files; rename it, or write `r#enum`"
    );
    let c = check_file_source(
        Kind::Cigar,
        "strength medium\nstick r#enum = 1\nexhale r#enum\n",
        None,
        None,
    );
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

/// A `.cig` file never gets a new error: a word that would be reserved at
/// medium is a hint at most.
#[test]
fn a_cig_file_with_a_keyword_name_is_not_refused() {
    let c = check_file_source(Kind::Cig, "stick match = 1\nexhale match\n", None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

/// cig-orchestrator's `orchestrate.cig`, read as a mild `.cigar` file, gave 63
/// refusals for words that were fields (45 `layer`, 18 `use`). It has none
/// now. The file is read from `CIG_ORCHESTRATOR_SRC` (the checkout; default
/// `../cig-orchestrator`), and the test says so and passes when it is not there.
#[test]
fn orchestrate_cig_as_cigar_has_no_reserved_word_errors() {
    let root = std::env::var("CIG_ORCHESTRATOR_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../cig-orchestrator"));
    let file = root.join("orchestrate.cig");
    let Ok(src) = std::fs::read_to_string(&file) else {
        eprintln!("note: skipping; {} is not there", file.display());
        return;
    };
    let c = check_file_source(
        Kind::Cigar,
        &src,
        Some(cigarscript::features::Strength::Mild),
        None,
    );
    let reserved: Vec<_> = c
        .errors
        .iter()
        .filter(|d| matches!(d.code, "E1101" | "E1102" | "E1103" | "E1105"))
        .map(|d| format!("{} {} at {:?}", d.code, d.message, d.line))
        .collect();
    assert!(reserved.is_empty(), "{}", reserved.join("\n"));
}

#[test]
fn binding_moves_at_full() {
    run("s5-");
}

#[test]
fn closure_capture_borrows_or_moves() {
    run("s6-");
}

#[test]
fn possibly_moved_is_moved() {
    run("s7-");
}
