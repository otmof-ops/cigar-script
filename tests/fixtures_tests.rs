// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! Fixture tests for issue #7's stated feature table.

mod common;

use cigarscript::features::Strength;
use common::{as_cig, as_cigar, cigscript_result, fingerprints};

fn read(path: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

#[test]
fn record_in_mild_is_e1101_with_the_hint() {
    let source = read("tests/fixtures/mild/record.cigar");
    let checked = as_cigar(&source, None);
    let d = checked
        .errors
        .iter()
        .find(|d| d.code == "E1101")
        .expect("expected E1101");
    assert!(d.message.contains("records"), "message: {}", d.message);
    assert!(
        d.message.contains("strength medium"),
        "message: {}",
        d.message
    );
    let hint = d.hint.as_deref().expect("expected hint");
    assert!(hint.contains("`strength medium`"), "hint: {hint}");
}

#[test]
fn record_in_medium_checks() {
    let source = read("tests/fixtures/medium/record.cigar");
    let checked = as_cigar(&source, None);
    assert!(checked.ok(), "errors: {:?}", checked.errors);
}

#[test]
fn foreign_in_full_is_e1102_naming_issue_5() {
    let source = read("tests/fixtures/full/foreign.cigar");
    let checked = as_cigar(&source, None);
    let d = checked
        .errors
        .iter()
        .find(|d| d.code == "E1102")
        .expect("expected E1102");
    assert!(d.message.contains("foreign"), "message: {}", d.message);
    assert!(d.message.contains("#5"), "message: {}", d.message);
}

#[test]
fn owned_in_medium_is_e1101_naming_full() {
    let source = read("tests/fixtures/medium/owned.cigar");
    let checked = as_cigar(&source, None);
    let d = checked
        .errors
        .iter()
        .find(|d| d.code == "E1101")
        .expect("expected E1101");
    assert!(
        d.message.contains("strength full"),
        "message: {}",
        d.message
    );
    assert!(d.message.contains("owned values"), "message: {}", d.message);
}

#[test]
fn stick_type_annotation_in_mild_is_e1101() {
    let source = read("tests/fixtures/mild/annotation_stick.cigar");
    let checked = as_cigar(&source, None);
    assert!(
        checked
            .errors
            .iter()
            .any(|d| d.code == "E1101" && d.message.contains("type annotations")),
        "want E1101 (type annotations): {:?}",
        fingerprints(&checked.errors)
    );
}

#[test]
fn pull_signature_gives_two_findings() {
    let source = read("tests/fixtures/mild/pull_signature.cigar");
    let checked = as_cigar(&source, None);
    let anns: Vec<_> = checked
        .errors
        .iter()
        .filter(|d| d.code == "E1101" && d.message.contains("type annotations"))
        .collect();
    assert_eq!(
        anns.len(),
        2,
        "want two type-annotation findings: {:?}",
        fingerprints(&checked.errors)
    );
    let cols: Vec<_> = anns.iter().filter_map(|d| d.col).collect();
    // The two annotation spans sit on distinct columns of line 1.
    assert_eq!(cols.len(), 2);
    assert_ne!(cols[0], cols[1]);
}

#[test]
fn turning_down_lists_every_medium_use_with_positions() {
    let source = read("tests/fixtures/medium/three_features.cigar");
    let checked = as_cigar(&source, Some(Strength::Mild));
    let above: Vec<_> = checked
        .errors
        .iter()
        .filter(|d| d.code == "E1101")
        .collect();
    assert!(
        above.len() >= 3,
        "want at least three E1101 findings: {:?}",
        fingerprints(&checked.errors)
    );
    for d in &above {
        assert!(d.line.is_some(), "every finding has a line");
        assert!(d.col.is_some(), "every finding has a column");
    }
    // Findings are ordered by position.
    let mut last = (0u32, 0u32);
    for d in &above {
        let here = (d.line.unwrap(), d.col.unwrap());
        assert!(here >= last, "findings not sorted: {here:?} after {last:?}");
        last = here;
    }
}

#[test]
fn a_late_declaration_is_e1103() {
    let source = read("tests/fixtures/declaration/late.cigar");
    let checked = as_cigar(&source, None);
    let d = checked
        .errors
        .iter()
        .find(|d| d.code == "E1103")
        .expect("expected E1103");
    assert_eq!(d.line, Some(3));
}

#[test]
fn strength_spicy_is_e1103() {
    let source = read("tests/fixtures/declaration/spicy.cigar");
    let checked = as_cigar(&source, None);
    assert!(
        checked.errors.iter().any(|d| d.code == "E1103"),
        "want E1103: {:?}",
        fingerprints(&checked.errors)
    );
}

#[test]
fn cig_file_starting_with_strength_medium_gets_cigscripts_own_result() {
    let source = read("tests/fixtures/cig/strength_medium.cig");
    let (cig_errors, cig_warnings) = cigscript_result(&source);
    let checked = as_cig(&source);
    assert_eq!(
        fingerprints(&cig_errors),
        fingerprints(&checked.errors),
        "errors differ from cig's"
    );
    assert_eq!(
        fingerprints(&cig_warnings),
        fingerprints(&checked.warnings),
        "warnings differ from cig's"
    );
}

/// A map a default value writes inside a `pull`'s parentheses is not a
/// type annotation: only `name: T` at the parameter level is.
#[test]
fn a_map_in_a_default_value_is_not_an_annotation() {
    let found = cigarscript::gate::scan(
        "pull f(a = {x: 1}, b = [{y: 2}]) {\n  snuff a\n}\n",
        cigarscript::features::Strength::Mild,
    )
    .expect("tokenizes");
    assert!(
        found.is_empty(),
        "{:?}",
        found.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

/// The messages read as sentences whatever the feature's name.
#[test]
fn e1101_reads_using_x_needs_strength() {
    let found = cigarscript::gate::scan(
        "record Move { from: 1 }\n",
        cigarscript::features::Strength::Mild,
    )
    .expect("tokenizes");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].message,
        "using records needs strength medium; this file is mild"
    );
}
