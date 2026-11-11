// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The generated feature table matches LANGUAGE.md row for row.

use cigarscript::features::{Detect, Feature, FEATURES};

fn table_from_language_md() -> Vec<(String, String, String, String, String)> {
    let md_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/LANGUAGE.md");
    let md = std::fs::read_to_string(&md_path).expect("read LANGUAGE.md");
    let begin = md.find("<!-- features:begin -->").expect("begin marker");
    let after = &md[begin + "<!-- features:begin -->".len()..];
    let end = after.find("<!-- features:end -->").expect("end marker");
    let body = &after[..end];
    let mut rows = Vec::new();
    let mut seen_header = false;
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('|') {
            continue;
        }
        if line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }
        if !seen_header {
            seen_header = true;
            continue;
        }
        let cells: Vec<String> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        assert_eq!(cells.len(), 5, "row shape: {line}");
        rows.push((
            cells[0].clone(),
            cells[1].clone(),
            cells[2].clone(),
            cells[3].clone(),
            cells[4].clone(),
        ));
    }
    rows
}

#[test]
fn the_feature_table_is_the_docs_table() {
    let md_rows = table_from_language_md();
    assert_eq!(
        md_rows.len(),
        FEATURES.len(),
        "the generated table has {} rows, LANGUAGE.md has {}",
        FEATURES.len(),
        md_rows.len()
    );
    for (i, (f, md)) in FEATURES.iter().zip(md_rows.iter()).enumerate() {
        let (name, strength, detect, issue, built) = md;
        assert_eq!(f.name, *name, "row {i}: name");
        assert_eq!(f.strength.word(), *strength, "row {i}: strength");
        let detect_word = match f.detect {
            Detect::Annotation => "annotation".to_string(),
            Detect::Word(w) => format!("word:{w}"),
        };
        assert_eq!(detect_word, *detect, "row {i}: detect");
        assert_eq!(f.issue, *issue, "row {i}: issue");
        let built_word = if f.built { "yes" } else { "no" };
        assert_eq!(built_word, built, "row {i}: built");
    }
}

#[test]
fn every_row_is_distinct_by_name() {
    let mut names: Vec<_> = FEATURES.iter().map(|f: &Feature| f.name).collect();
    names.sort();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "duplicate feature names");
}
